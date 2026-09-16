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
    pub si: bool,
    pub apparent_size: bool,
    pub dirs_first: bool,
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
            si: false,
            apparent_size: false,
            dirs_first: false,
            show_hidden: true,
            show_count: false,
            show_mtime: false,
            bar_mode: BarMode::BarPercent,
            bar_width: 24,
            sort: SortKey::Size,
            sort_reverse: false,
            exclude: Vec::new(),
            one_file_system: false,
            threads: 0,
            confirm_delete: true,
            read_only: false,
            date_format: "%Y-%m-%d %H:%M".into(),
        }
    }
}

/// `$XDG_CONFIG_HOME/cdu` (or the platform equivalent).
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("cdu"))
}

pub fn default_config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("config.toml"))
}

pub fn themes_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("themes"))
}

impl Config {
    /// Load from an explicit path, or from the default location if it exists.
    /// A missing default file yields the built-in defaults.
    pub fn load(explicit: Option<&Path>) -> Result<Config> {
        let path = match explicit {
            Some(p) => p.to_path_buf(),
            None => match default_config_path() {
                Some(p) if p.is_file() => p,
                _ => return Ok(Config::default()),
            },
        };
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading config {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))
    }
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
        assert_eq!(parsed.bar_mode, def.bar_mode);
        assert_eq!(parsed.bar_width, def.bar_width);
        assert_eq!(parsed.sort, def.sort);
        assert_eq!(parsed.date_format, def.date_format);
        assert_eq!(parsed.show_hidden, def.show_hidden);
    }

    #[test]
    fn partial_config_uses_defaults() {
        let c: Config = toml::from_str("theme = \"nord\"\nbar-width = 10\n").unwrap();
        assert_eq!(c.theme, "nord");
        assert_eq!(c.bar_width, 10);
        assert!(c.mouse);
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
