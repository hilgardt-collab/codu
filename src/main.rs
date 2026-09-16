//! cdu — colourful, themeable, navigable disk usage TUI.

mod app;
mod cli;
mod config;
mod format;
mod icons;
mod scan;
mod theme;
mod theme_editor;
mod ui;

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use ratatui::crossterm::execute;

use crate::app::App;
use crate::cli::{Cli, IconArg};
use crate::config::{Config, DEFAULT_CONFIG_TOML};
use crate::icons::{IconMode, IconSet};
use crate::scan::ScanOptions;
use crate::theme::{BUILTIN, Theme, ThemeSource, available_themes};

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.dump_config {
        print!("{DEFAULT_CONFIG_TOML}");
        return Ok(());
    }
    if let Some(name) = &cli.dump_theme {
        return dump_theme(name);
    }
    if cli.list_themes {
        return list_themes();
    }

    let mut config = Config::load(cli.config.as_deref())?;
    apply_cli_overrides(&mut config, &cli);

    let mut warnings: Vec<String> = Vec::new();
    let theme = match Theme::load(&config.theme) {
        Ok(t) => t,
        Err(e) => {
            warnings.push(format!("{e:#}; falling back to catppuccin-mocha"));
            Theme::load("catppuccin-mocha")?
        }
    };
    for w in &theme.warnings {
        warnings.push(format!("theme {}: {w}", theme.id));
    }
    let icons = IconSet::build(config.icons, &theme.icons);

    let root = cli.path.clone().unwrap_or_else(|| PathBuf::from("."));
    let root =
        std::path::absolute(&root).with_context(|| format!("resolving {}", root.display()))?;
    let root = normalize_trailing(root);
    if !root.is_dir() {
        bail!("{} is not a directory", root.display());
    }

    let scan_options = ScanOptions {
        one_file_system: config.one_file_system,
        excludes: build_globset(&config.exclude)?,
    };
    if config.threads > 0 {
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(config.threads)
            .build_global();
    }

    let mouse = config.mouse;
    let mut app = App::new(config, theme, icons, root, scan_options);
    if !warnings.is_empty() {
        app.set_status(warnings.join(" | "), true);
    }

    let mut terminal = ratatui::init();
    if mouse {
        let _ = execute!(io::stdout(), EnableMouseCapture);
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = execute!(io::stdout(), DisableMouseCapture);
            previous(info);
        }));
    }
    let result = run(&mut terminal, &mut app);
    if mouse {
        let _ = execute!(io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;
        if app.should_quit {
            return Ok(());
        }
        let timeout = if app.is_scanning() || app.status.is_some() {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(250)
        };
        if event::poll(timeout)? {
            handle(app, event::read()?);
            // Drain anything else already queued (fast key repeat, wheel bursts).
            while event::poll(Duration::ZERO)? {
                handle(app, event::read()?);
            }
        }
        app.on_tick();
    }
}

fn handle(app: &mut App, ev: Event) {
    match ev {
        Event::Key(k) => app.on_key(k),
        Event::Mouse(m) => app.on_mouse(m),
        _ => {}
    }
}

fn apply_cli_overrides(config: &mut Config, cli: &Cli) {
    if let Some(t) = &cli.theme {
        config.theme = t.clone();
    }
    if let Some(i) = cli.icons {
        config.icons = match i {
            IconArg::Emoji => IconMode::Emoji,
            IconArg::Ascii => IconMode::Ascii,
            IconArg::None => IconMode::None,
        };
    }
    if cli.tree {
        config.view = config::View::Tree;
    }
    if cli.one_file_system {
        config.one_file_system = true;
    }
    config.exclude.extend(cli.exclude.iter().cloned());
    if cli.apparent_size {
        config.apparent_size = true;
    }
    if cli.si {
        config.si = true;
    }
    if cli.read_only {
        config.read_only = true;
    }
    if cli.no_mouse {
        config.mouse = false;
    }
    if let Some(t) = cli.threads {
        config.threads = t;
    }
}

fn build_globset(patterns: &[String]) -> Result<Option<globset::GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut b = globset::GlobSetBuilder::new();
    for p in patterns {
        b.add(globset::Glob::new(p).with_context(|| format!("bad exclude pattern `{p}`"))?);
    }
    Ok(Some(b.build()?))
}

/// Strip a trailing separator so `file_name()` works on the root, but keep `/`.
fn normalize_trailing(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    if s.len() > 1 && s.ends_with(std::path::MAIN_SEPARATOR) {
        PathBuf::from(s.trim_end_matches(std::path::MAIN_SEPARATOR))
    } else {
        p
    }
}

fn dump_theme(name: &str) -> Result<()> {
    if let Some((_, text)) = BUILTIN.iter().find(|(id, _)| *id == name) {
        print!("{text}");
        return Ok(());
    }
    if let Some(dir) = config::themes_dir() {
        let p = dir.join(format!("{name}.toml"));
        if p.is_file() {
            print!("{}", std::fs::read_to_string(&p)?);
            return Ok(());
        }
    }
    Err(anyhow!("unknown theme `{name}` (try --list-themes)"))
}

fn list_themes() -> Result<()> {
    let mut out = io::stdout().lock();
    for t in available_themes() {
        let source = match &t.source {
            ThemeSource::BuiltIn => "built-in".to_string(),
            ThemeSource::User(p) => p.display().to_string(),
        };
        writeln!(out, "{:<20} {:<28} {}", t.id, t.name, source)?;
    }
    if let Some(dir) = config::themes_dir() {
        writeln!(out, "\nUser themes directory: {}", dir.display())?;
    }
    Ok(())
}
