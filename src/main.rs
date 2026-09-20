//! codu — colourful, themeable, navigable disk usage TUI.

mod app;
mod cli;
mod config;
mod format;
mod icons;
mod scan;
mod theme;
mod theme_editor;
mod ui;
mod volumes;

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
        emit(DEFAULT_CONFIG_TOML);
        return Ok(());
    }
    if let Some(name) = &cli.dump_theme {
        return dump_theme(name);
    }
    if cli.list_themes {
        return list_themes();
    }
    if let Some(shell) = cli.shell {
        emit(&shell_integration(shell));
        return Ok(());
    }

    let (mut config, mut warnings) = Config::load(cli.config.as_deref())?;
    apply_cli_overrides(&mut config, &cli);
    if let Err(e) = format::check_date_format(&config.date_format) {
        warnings.push(format!(
            "date-format `{}` {e}; using the default",
            config.date_format
        ));
        config.date_format = Config::default().date_format;
    }

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
    let start_dir = std::env::current_dir().unwrap_or_else(|_| root.clone());
    let mut app = App::new(config, theme, icons, root, scan_options, cli.volumes);
    if !warnings.is_empty() {
        // A status line would be overwritten by "scanned N items" moments
        // later; a popup stays until dismissed.
        app.show_startup_warnings(&warnings);
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

    if app.config.cd_on_exit && !cli.no_cd {
        cd_on_exit(&app, cli.cwd_file.as_deref(), &start_dir);
    }
    result
}

/// Write to stdout, ignoring a closed pipe (`codu --shell bash | head`).
fn emit(text: &str) {
    let mut out = io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

/// Hand the shown directory to the shell wrapper, or explain how to set it up
/// when the user navigated somewhere without the wrapper being installed.
fn cd_on_exit(app: &App, cwd_file: Option<&std::path::Path>, start_dir: &std::path::Path) {
    let Some(dir) = app.shown_directory() else {
        return;
    };
    match cwd_file {
        Some(file) => {
            if let Err(e) = std::fs::write(file, dir.as_os_str().as_encoded_bytes()) {
                eprintln!("codu: could not write {}: {e}", file.display());
            }
        }
        None => {
            if dir != start_dir && std::io::IsTerminal::is_terminal(&io::stderr()) {
                let shell = std::env::var("SHELL")
                    .ok()
                    .and_then(|s| s.rsplit('/').next().map(str::to_string))
                    .filter(|s| matches!(s.as_str(), "bash" | "zsh" | "fish"))
                    .unwrap_or_else(|| "bash".to_string());
                // A directory name could carry terminal escape sequences;
                // the UI strips them and so does this hint.
                eprintln!(
                    "codu: you were in {}",
                    scan::sanitize(&dir.to_string_lossy())
                );
                eprintln!(
                    "codu: to land there on exit, add to your shell config:  eval \"$(codu --shell {shell})\"   (or set cd-on-exit = false)"
                );
            }
        }
    }
}

/// Shell function that runs codu with a temp cwd file and cds into the result.
fn shell_integration(shell: cli::ShellArg) -> String {
    match shell {
        cli::ShellArg::Bash | cli::ShellArg::Zsh => {
            r#"# codu shell integration: run `eval "$(codu --shell bash)"` from your rc file.
codu() {
    local tmp cwd rc
    tmp="$(mktemp -t codu-cwd.XXXXXX)" || return
    command codu --cwd-file="$tmp" "$@"
    rc=$?
    cwd="$(cat -- "$tmp" 2>/dev/null)"
    rm -f -- "$tmp"
    if [ -n "$cwd" ] && [ "$cwd" != "$PWD" ] && [ -d "$cwd" ]; then
        builtin cd -- "$cwd" || return
    fi
    return $rc
}
"#
            .to_string()
        }
        cli::ShellArg::Fish => {
            r#"# codu shell integration: run `codu --shell fish | source` from config.fish.
function codu --wraps codu --description 'codu, changing directory on exit'
    set -l tmp (mktemp -t codu-cwd.XXXXXX); or return
    command codu --cwd-file="$tmp" $argv
    set -l rc $status
    set -l cwd (cat -- "$tmp" 2>/dev/null)
    rm -f -- "$tmp"
    if test -n "$cwd"; and test "$cwd" != "$PWD"; and test -d "$cwd"
        builtin cd -- "$cwd"
    end
    return $rc
end
"#
            .to_string()
        }
    }
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;
        if app.should_quit {
            return Ok(());
        }
        // Only a running scan, a background volume lookup or a status message
        // that has to expire needs a timer; otherwise sleep until input arrives.
        let timeout = if app.is_scanning() || app.volumes_loading() || app.status.is_some() {
            Duration::from_millis(50)
        } else {
            Duration::from_secs(1)
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
    if cli.cross_volumes {
        config.one_file_system = false;
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
/// Rebuilding from components keeps a non-UTF-8 path intact.
fn normalize_trailing(p: PathBuf) -> PathBuf {
    p.components().collect()
}

fn dump_theme(name: &str) -> Result<()> {
    if let Some((_, text)) = BUILTIN.iter().find(|(id, _)| *id == name) {
        emit(text);
        return Ok(());
    }
    if let Some(dir) = config::themes_dir() {
        let p = dir.join(format!("{name}.toml"));
        if p.is_file() {
            emit(&std::fs::read_to_string(&p)?);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_separator_is_stripped_but_root_is_kept() {
        assert_eq!(
            normalize_trailing(PathBuf::from("/a/b/")),
            PathBuf::from("/a/b")
        );
        assert_eq!(
            normalize_trailing(PathBuf::from("/a/b")),
            PathBuf::from("/a/b")
        );
        assert_eq!(normalize_trailing(PathBuf::from("/")), PathBuf::from("/"));
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_root_survives_normalisation() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let p = PathBuf::from(OsStr::from_bytes(b"/tmp/bad\xff/"));
        assert_eq!(
            normalize_trailing(p).as_os_str().as_bytes(),
            b"/tmp/bad\xff"
        );
    }

    #[test]
    fn shell_wrappers_pass_cwd_file_before_user_arguments() {
        for shell in [cli::ShellArg::Bash, cli::ShellArg::Fish] {
            let text = shell_integration(shell);
            let line = text.lines().find(|l| l.contains("command codu")).unwrap();
            let cwd = line.find("--cwd-file").unwrap();
            let args = line.find("$@").or_else(|| line.find("$argv")).unwrap();
            assert!(cwd < args, "{line}");
        }
    }
}
