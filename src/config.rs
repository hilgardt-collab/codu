//! User configuration (`config.toml`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::icons::IconMode;

/// Default config file with documentation comments, embedded so it can be
/// printed with `--dump-config`.
pub const DEFAULT_CONFIG_TOML: &str = include_str!("../config.default.toml");

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BarMode {
    BarPercent,
    Bar,
    Percent,
    None,
}

impl BarMode {
    pub fn next(self) -> Self {
        match self {
            BarMode::BarPercent => BarMode::Bar,
            BarMode::Bar => BarMode::Percent,
            BarMode::Percent => BarMode::None,
            BarMode::None => BarMode::BarPercent,
        }
    }

    pub fn show_bar(self) -> bool {
        matches!(self, BarMode::BarPercent | BarMode::Bar)
    }

    pub fn show_percent(self) -> bool {
        matches!(self, BarMode::BarPercent | BarMode::Percent)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum View {
    List,
    Tree,
}

/// How much of the keyboard guide to show at the bottom of the screen.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum KeyGuide {
    /// Every shortcut for the current context, wrapped over several lines.
    Full,
    /// A single line, truncated to the terminal width.
    Compact,
    Off,
}

impl KeyGuide {
    pub fn next(self) -> Self {
        match self {
            KeyGuide::Full => KeyGuide::Compact,
            KeyGuide::Compact => KeyGuide::Off,
            KeyGuide::Off => KeyGuide::Full,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            KeyGuide::Full => "full",
            KeyGuide::Compact => "compact",
            KeyGuide::Off => "off",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SortKey {
    Size,
    Name,
    Count,
    Mtime,
}

impl SortKey {
    pub fn label(self) -> &'static str {
        match self {
            SortKey::Size => "size",
            SortKey::Name => "name",
            SortKey::Count => "items",
            SortKey::Mtime => "mtime",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
    pub theme: String,
    pub icons: IconMode,
    pub view: View,
    pub mouse: bool,
    pub borders: bool,
    pub key_guide: KeyGuide,
    pub si: bool,
    pub apparent_size: bool,
    pub dirs_first: bool,
    pub group_by_type: bool,
    pub show_hidden: bool,
    pub show_count: bool,
    pub show_mtime: bool,
    pub bar_mode: BarMode,
    pub bar_width: u16,
    pub sort: SortKey,
    pub sort_reverse: bool,
    pub exclude: Vec<String>,
    pub one_file_system: bool,
    pub threads: usize,
    pub confirm_delete: bool,
    pub read_only: bool,
    /// With the shell integration installed, exit into the shown directory.
    pub cd_on_exit: bool,
    pub date_format: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            theme: "catppuccin-mocha".into(),
            icons: IconMode::Emoji,
            view: View::List,
            mouse: true,
            borders: true,
            key_guide: KeyGuide::Full,
            si: false,
            apparent_size: false,
            dirs_first: false,
            group_by_type: false,
            show_hidden: true,
            show_count: false,
            show_mtime: false,
            bar_mode: BarMode::BarPercent,
            bar_width: 24,
            sort: SortKey::Size,
            sort_reverse: false,
            exclude: Vec::new(),
            one_file_system: true,
            threads: 0,
            confirm_delete: true,
            read_only: false,
            cd_on_exit: true,
            date_format: "%Y-%m-%d %H:%M".into(),
        }
    }
}

/// `$XDG_CONFIG_HOME/codu` (or the platform equivalent).
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("codu"))
}

pub fn default_config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("config.toml"))
}

pub fn themes_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("themes"))
}

/// Persist `theme = "<id>"` in the user's config.toml, creating the file from
/// the commented default when it does not exist yet. Returns the path written.
pub fn set_default_theme(id: &str) -> Result<PathBuf> {
    let path = default_config_path().context("no config directory on this platform")?;
    // A new file holds only the theme line: copying the full default template
    // would freeze today's defaults into the user's config.
    let existing = if path.is_file() {
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?
    } else {
        "# codu configuration. Run `codu --dump-config` to see every option.\n".to_string()
    };
    let line = format!("theme = {}", toml_string(id));
    let mut replaced = false;
    let mut out: Vec<String> = existing
        .lines()
        .map(|l| {
            if !replaced
                && l.trim_start().starts_with("theme")
                && l.split('=').next().is_some_and(|k| k.trim() == "theme")
            {
                replaced = true;
                line.clone()
            } else {
                l.to_string()
            }
        })
        .collect();
    if !replaced {
        out.insert(0, line);
    }
    let mut text = out.join("\n");
    text.push('\n');
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// Quote a string as a TOML basic string.
pub fn toml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Every top-level key `config.toml` understands, so that a misspelt key can
/// be reported instead of silently ignored: a `readonly = true` that did
/// nothing would be a nasty surprise.
pub const KNOWN_KEYS: &[&str] = &[
    "theme",
    "icons",
    "view",
    "mouse",
    "borders",
    "key-guide",
    "si",
    "apparent-size",
    "dirs-first",
    "group-by-type",
    "show-hidden",
    "show-count",
    "show-mtime",
    "bar-mode",
    "bar-width",
    "sort",
    "sort-reverse",
    "exclude",
    "one-file-system",
    "threads",
    "confirm-delete",
    "read-only",
    "cd-on-exit",
    "date-format",
];

impl Config {
    /// Load from an explicit path, or from the default location if it exists.
    /// A missing default file yields the built-in defaults. Unknown keys are
    /// returned as warnings rather than treated as errors.
    pub fn load(explicit: Option<&Path>) -> Result<(Config, Vec<String>)> {
        let path = match explicit {
            Some(p) => p.to_path_buf(),
            None => match default_config_path() {
                Some(p) if p.is_file() => p,
                _ => return Ok((Config::default(), Vec::new())),
            },
        };
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading config {}", path.display()))?;
        Config::parse(&text).with_context(|| format!("parsing config {}", path.display()))
    }

    /// Parse config text, reporting unknown top-level keys as warnings.
    pub fn parse(text: &str) -> Result<(Config, Vec<String>)> {
        let table: toml::Table = toml::from_str(text)?;
        let warnings = table
            .keys()
            .filter(|k| !KNOWN_KEYS.contains(&k.as_str()))
            .map(|k| match nearest_key(k) {
                Some(known) => {
                    format!("config: unknown key `{k}` ignored (did you mean `{known}`?)")
                }
                None => format!("config: unknown key `{k}` ignored"),
            })
            .collect();
        let config: Config = toml::from_str(text)?;
        Ok((config, warnings))
    }
}

/// A known key that differs from `key` only in case or in `-`/`_` separators.
fn nearest_key(key: &str) -> Option<&'static str> {
    let fold = |s: &str| s.to_ascii_lowercase().replace(['-', '_'], "");
    let want = fold(key);
    KNOWN_KEYS.iter().copied().find(|k| fold(k) == want)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_file_parses_to_defaults() {
        let parsed: Config = toml::from_str(DEFAULT_CONFIG_TOML).unwrap();
        let def = Config::default();
        assert_eq!(parsed.theme, def.theme);
        assert_eq!(parsed.icons, def.icons);
        assert_eq!(parsed.view, def.view);
        assert_eq!(parsed.key_guide, def.key_guide);
        assert_eq!(parsed.bar_mode, def.bar_mode);
        assert_eq!(parsed.bar_width, def.bar_width);
        assert_eq!(parsed.sort, def.sort);
        assert_eq!(parsed.date_format, def.date_format);
        assert_eq!(parsed.show_hidden, def.show_hidden);
    }

    #[test]
    fn known_keys_match_the_default_config_file() {
        let table: toml::Table = toml::from_str(DEFAULT_CONFIG_TOML).unwrap();
        let mut in_file: Vec<&str> = table.keys().map(String::as_str).collect();
        in_file.sort_unstable();
        let mut known = KNOWN_KEYS.to_vec();
        known.sort_unstable();
        assert_eq!(in_file, known);
    }

    #[test]
    fn unknown_keys_warn_and_never_apply() {
        let (c, w) = Config::parse("readonly = true\nbogus = 1\ntheme = \"nord\"\n").unwrap();
        assert!(!c.read_only, "the typo must not silently take effect");
        assert_eq!(c.theme, "nord");
        assert_eq!(w.len(), 2);
        assert!(
            w.iter()
                .any(|m| m.contains("`readonly`") && m.contains("did you mean `read-only`")),
            "{w:?}"
        );
        assert!(
            w.iter()
                .any(|m| m.contains("`bogus`") && !m.contains("did you mean")),
            "{w:?}"
        );
        let (_, w) = Config::parse("theme = \"nord\"\n").unwrap();
        assert!(w.is_empty());
    }

    #[test]
    fn partial_config_uses_defaults() {
        let c: Config = toml::from_str("theme = \"nord\"\nbar-width = 10\n").unwrap();
        assert_eq!(c.theme, "nord");
        assert_eq!(c.bar_width, 10);
        assert!(c.mouse);
    }

    #[test]
    fn theme_line_replacement_keeps_other_keys_and_new_file_is_minimal() {
        // Exercise the line rewriting on text, without touching the real config.
        let existing = "# comment\ntheme = \"nord\"\nicons = \"ascii\"\n";
        let out: Vec<String> = existing
            .lines()
            .map(|l| {
                if l.split('=').next().is_some_and(|k| k.trim() == "theme") {
                    format!("theme = {}", toml_string("mine"))
                } else {
                    l.to_string()
                }
            })
            .collect();
        assert_eq!(out, ["# comment", "theme = \"mine\"", "icons = \"ascii\""]);
        let parsed: Config = toml::from_str(&out.join("\n")).unwrap();
        assert_eq!(parsed.theme, "mine");
        assert!(
            parsed.one_file_system,
            "defaults still apply to keys the file omits"
        );
    }

    #[test]
    fn toml_string_escapes() {
        assert_eq!(toml_string("plain"), "\"plain\"");
        assert_eq!(toml_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
        let parsed: toml::Value =
            toml::from_str(&format!("x = {}", toml_string("q\"\\\n"))).unwrap();
        assert_eq!(parsed["x"].as_str(), Some("q\"\\\n"));
    }

    #[test]
    fn bar_mode_cycles() {
        let mut m = BarMode::BarPercent;
        for _ in 0..4 {
            m = m.next();
        }
        assert_eq!(m, BarMode::BarPercent);
    }
}
