//! Theme model: colours, styles, bar glyphs and icon overrides loaded from
//! TOML. Built-in themes are embedded; user themes live in the config dir.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

/// Built-in themes, embedded at compile time. The first entry is the fallback
/// base that every other theme is layered on top of.
pub const BUILTIN: &[(&str, &str)] = &[
    ("ansi", include_str!("../themes/ansi.toml")),
    (
        "catppuccin-mocha",
        include_str!("../themes/catppuccin-mocha.toml"),
    ),
    (
        "catppuccin-latte",
        include_str!("../themes/catppuccin-latte.toml"),
    ),
    ("dracula", include_str!("../themes/dracula.toml")),
    ("gruvbox-dark", include_str!("../themes/gruvbox-dark.toml")),
    ("nord", include_str!("../themes/nord.toml")),
    ("tokyo-night", include_str!("../themes/tokyo-night.toml")),
    (
        "solarized-light",
        include_str!("../themes/solarized-light.toml"),
    ),
];

// ---------------------------------------------------------------------------
// Raw file model

/// The raw, editable form of a theme: exactly what the TOML file holds.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ThemeDoc {
    pub name: Option<String>,
    pub dark: Option<bool>,
    #[serde(default)]
    pub palette: BTreeMap<String, String>,
    #[serde(default)]
    pub styles: BTreeMap<String, StyleDef>,
    #[serde(default)]
    pub bar: BarDef,
    #[serde(default)]
    pub icons: IconsDef,
}

impl ThemeDoc {
    /// Serialize in the same compact layout as the built-in theme files.
    pub fn to_toml(&self) -> String {
        use crate::config::toml_string as q;
        let mut out = String::new();
        if let Some(n) = &self.name {
            out.push_str(&format!("name = {}\n", q(n)));
        }
        if let Some(d) = self.dark {
            out.push_str(&format!("dark = {d}\n"));
        }
        if !self.palette.is_empty() {
            out.push_str("\n[palette]\n");
            for (k, v) in &self.palette {
                out.push_str(&format!("{} = {}\n", key(k), q(v)));
            }
        }
        if !self.styles.is_empty() {
            out.push_str("\n[styles]\n");
            for (k, def) in &self.styles {
                out.push_str(&format!("{:<14}= {}\n", key(k), def.to_inline()));
            }
        }
        {
            let b = &self.bar;
            let mut lines = Vec::new();
            if let Some(v) = &b.filled {
                lines.push(format!("filled   = {}", q(v)));
            }
            if let Some(v) = &b.empty {
                lines.push(format!("empty    = {}", q(v)));
            }
            if let Some(v) = b.gradient {
                lines.push(format!("gradient = {v}"));
            }
            if let Some(v) = &b.low {
                lines.push(format!("low      = {}", q(v)));
            }
            if let Some(v) = &b.mid {
                lines.push(format!("mid      = {}", q(v)));
            }
            if let Some(v) = &b.high {
                lines.push(format!("high     = {}", q(v)));
            }
            if !lines.is_empty() {
                out.push_str("\n[bar]\n");
                for l in lines {
                    out.push_str(&l);
                    out.push('\n');
                }
            }
        }
        {
            let i = &self.icons;
            let mut lines = Vec::new();
            for (name, v) in [
                ("app", &i.app),
                ("dir", &i.dir),
                ("dir_open", &i.dir_open),
                ("parent", &i.parent),
                ("file", &i.file),
                ("symlink", &i.symlink),
                ("hidden", &i.hidden),
                ("special", &i.special),
                ("error", &i.error),
                ("empty_dir", &i.empty_dir),
                ("mount", &i.mount),
                ("volume", &i.volume),
                ("removable", &i.removable),
                ("network", &i.network),
                ("optical", &i.optical),
                ("swap", &i.swap),
                ("unmounted", &i.unmounted),
            ] {
                if let Some(v) = v {
                    lines.push(format!("{name} = {}", q(v)));
                }
            }
            if let Some(sp) = &i.spinner {
                let items: Vec<String> = sp.iter().map(|f| q(f)).collect();
                lines.push(format!("spinner = [{}]", items.join(", ")));
            }
            if !lines.is_empty() {
                out.push_str("\n[icons]\n");
                for l in lines {
                    out.push_str(&l);
                    out.push('\n');
                }
            }
            if !i.ext.is_empty() {
                out.push_str("\n[icons.ext]\n");
                for (k, v) in &i.ext {
                    out.push_str(&format!("{} = {}\n", key(k), q(v)));
                }
            }
            if !i.names.is_empty() {
                out.push_str("\n[icons.names]\n");
                for (k, v) in &i.names {
                    out.push_str(&format!("{} = {}\n", key(k), q(v)));
                }
            }
        }
        out
    }
}

/// A TOML key: bare when possible, quoted otherwise.
fn key(k: &str) -> String {
    if !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        k.to_string()
    } else {
        crate::config::toml_string(k)
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StyleDef {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub bold: Option<bool>,
    pub dim: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub reversed: Option<bool>,
    pub crossed_out: Option<bool>,
}

impl StyleDef {
    /// `{ fg = "...", bold = true }` as written in theme files.
    pub fn to_inline(&self) -> String {
        use crate::config::toml_string as q;
        let mut parts = Vec::new();
        if let Some(v) = &self.fg {
            parts.push(format!("fg = {}", q(v)));
        }
        if let Some(v) = &self.bg {
            parts.push(format!("bg = {}", q(v)));
        }
        for (name, v) in [
            ("bold", self.bold),
            ("dim", self.dim),
            ("italic", self.italic),
            ("underline", self.underline),
            ("reversed", self.reversed),
            ("crossed_out", self.crossed_out),
        ] {
            if let Some(v) = v {
                parts.push(format!("{name} = {v}"));
            }
        }
        if parts.is_empty() {
            "{}".to_string()
        } else {
            format!("{{ {} }}", parts.join(", "))
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarDef {
    pub filled: Option<String>,
    pub empty: Option<String>,
    pub gradient: Option<bool>,
    pub low: Option<String>,
    pub mid: Option<String>,
    pub high: Option<String>,
}

/// Icon overrides. Every field is optional; the built-in set fills the gaps.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IconsDef {
    pub app: Option<String>,
    pub dir: Option<String>,
    pub dir_open: Option<String>,
    pub parent: Option<String>,
    pub file: Option<String>,
    pub symlink: Option<String>,
    pub hidden: Option<String>,
    pub special: Option<String>,
    pub error: Option<String>,
    pub empty_dir: Option<String>,
    pub mount: Option<String>,
    pub volume: Option<String>,
    pub removable: Option<String>,
    pub network: Option<String>,
    pub optical: Option<String>,
    pub swap: Option<String>,
    pub unmounted: Option<String>,
    pub spinner: Option<Vec<String>>,
    #[serde(default)]
    pub ext: BTreeMap<String, String>,
    #[serde(default)]
    pub names: BTreeMap<String, String>,
}

// ---------------------------------------------------------------------------
// Resolved model

macro_rules! define_styles {
    ($($name:ident),* $(,)?) => {
        /// Every themeable UI element.
        #[derive(Clone, Debug, Default)]
        pub struct Styles {
            $(pub $name: Style,)*
        }

        impl Styles {
            pub const KEYS: &'static [&'static str] = &[$(stringify!($name)),*];

            fn set(&mut self, key: &str, style: Style) -> bool {
                match key {
                    $(stringify!($name) => { self.$name = style; true })*
                    _ => false,
                }
            }

            /// Look a style up by its theme-file key.
            pub fn get(&self, key: &str) -> Option<Style> {
                match key {
                    $(stringify!($name) => Some(self.$name),)*
                    _ => None,
                }
            }
        }
    };
}

define_styles! {
    background,
    header, header_title, header_path, header_theme,
    border, border_title,
    selected, marker,
    dir, file, symlink, special, hidden, excluded, error,
    flag,
    size, size_unit, percent, count, mtime,
    bar_filled, bar_empty,
    status, status_accent, keybar_key, keybar_label,
    popup, popup_border, popup_title, help_key, help_desc,
    danger, warning, success,
    spinner, scan_label, scan_value, scan_path,
    filter,
    tree_guide, tree_toggle,
    group,
    mount,
}

#[derive(Clone, Debug)]
pub struct BarTheme {
    pub filled: String,
    pub empty: String,
    pub gradient: bool,
    pub low: Color,
    pub mid: Color,
    pub high: Color,
}

impl Default for BarTheme {
    fn default() -> Self {
        BarTheme {
            filled: "█".into(),
            empty: "░".into(),
            gradient: false,
            low: Color::Green,
            mid: Color::Yellow,
            high: Color::Red,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub dark: Option<bool>,
    pub styles: Styles,
    pub bar: BarTheme,
    pub icons: IconsDef,
    /// Non-fatal problems found while loading (unknown keys, bad colours).
    pub warnings: Vec<String>,
}

impl Theme {
    /// Parse a theme from TOML text. `base` supplies values for any key the
    /// text does not define (pass `None` only for the root `ansi` theme).
    pub fn parse(id: &str, text: &str, base: Option<&Theme>) -> Result<Theme> {
        let doc: ThemeDoc = toml::from_str(text).context("parsing theme")?;
        Ok(Theme::from_doc(id, &doc, base))
    }

    /// Resolve an editable document into a theme. Never fails: problems
    /// become warnings and the affected keys keep their base values.
    pub fn from_doc(id: &str, file: &ThemeDoc, base: Option<&Theme>) -> Theme {
        let mut warnings = Vec::new();

        let mut styles = base.map(|b| b.styles.clone()).unwrap_or_default();
        for (key, def) in &file.styles {
            match build_style(def, &file.palette) {
                Ok(style) => {
                    if !styles.set(key, style) {
                        warnings.push(format!("unknown style key `{key}`"));
                    }
                }
                Err(e) => warnings.push(format!("style `{key}`: {e}")),
            }
        }

        let mut bar = base.map(|b| b.bar.clone()).unwrap_or_default();
        if let Some(f) = &file.bar.filled {
            bar.filled = single_cell(f).unwrap_or_else(|| {
                warnings.push(format!("bar.filled `{f}` is not a single-cell glyph"));
                bar.filled.clone()
            });
        }
        if let Some(e) = &file.bar.empty {
            bar.empty = single_cell(e).unwrap_or_else(|| {
                warnings.push(format!("bar.empty `{e}` is not a single-cell glyph"));
                bar.empty.clone()
            });
        }
        if let Some(g) = file.bar.gradient {
            bar.gradient = g;
        }
        for (label, value, slot) in [
            ("low", &file.bar.low, &mut bar.low),
            ("mid", &file.bar.mid, &mut bar.mid),
            ("high", &file.bar.high, &mut bar.high),
        ] {
            if let Some(v) = value {
                match parse_color(v, &file.palette) {
                    Ok(c) => *slot = c,
                    Err(e) => warnings.push(format!("bar.{label}: {e}")),
                }
            }
        }

        let mut icons = base.map(|b| b.icons.clone()).unwrap_or_default();
        merge_icons(&mut icons, file.icons.clone());

        Theme {
            id: id.to_string(),
            name: file.name.clone().unwrap_or_else(|| id.to_string()),
            dark: file.dark.or(base.and_then(|b| b.dark)),
            styles,
            bar,
            icons,
            warnings,
        }
    }

    /// Load the editable document for a theme name, path, or built-in id.
    pub fn load_doc(name: &str) -> Result<(String, ThemeDoc)> {
        let as_path = Path::new(name);
        if name.ends_with(".toml") || name.contains(std::path::MAIN_SEPARATOR) {
            let text = std::fs::read_to_string(as_path)
                .with_context(|| format!("reading theme {}", as_path.display()))?;
            let id = as_path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| name.to_string());
            return Ok((id, toml::from_str(&text).context("parsing theme")?));
        }
        if let Some(dir) = crate::config::themes_dir() {
            let candidate = dir.join(format!("{name}.toml"));
            if candidate.is_file() {
                let text = std::fs::read_to_string(&candidate)
                    .with_context(|| format!("reading theme {}", candidate.display()))?;
                return Ok((
                    name.to_string(),
                    toml::from_str(&text).context("parsing theme")?,
                ));
            }
        }
        if let Some((_, text)) = BUILTIN.iter().find(|(id, _)| *id == name) {
            return Ok((
                name.to_string(),
                toml::from_str(text).context("parsing theme")?,
            ));
        }
        Err(anyhow!("unknown theme `{name}` (try --list-themes)"))
    }

    /// The `ansi` theme: no truecolor, inherits the terminal palette.
    pub fn base() -> Theme {
        Theme::parse("ansi", BUILTIN[0].1, None).expect("built-in ansi theme must parse")
    }

    /// Resolve a theme by name (built-in or user) or by file path.
    pub fn load(name: &str) -> Result<Theme> {
        let base = Theme::base();
        if name == "ansi" {
            return Ok(base);
        }
        let as_path = Path::new(name);
        if name.ends_with(".toml") || name.contains(std::path::MAIN_SEPARATOR) {
            let text = std::fs::read_to_string(as_path)
                .with_context(|| format!("reading theme {}", as_path.display()))?;
            let id = as_path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| name.to_string());
            return Theme::parse(&id, &text, Some(&base));
        }
        if let Some(dir) = crate::config::themes_dir() {
            let candidate = dir.join(format!("{name}.toml"));
            if candidate.is_file() {
                let text = std::fs::read_to_string(&candidate)
                    .with_context(|| format!("reading theme {}", candidate.display()))?;
                return Theme::parse(name, &text, Some(&base));
            }
        }
        if let Some((_, text)) = BUILTIN.iter().find(|(id, _)| *id == name) {
            return Theme::parse(name, text, Some(&base));
        }
        Err(anyhow!("unknown theme `{name}` (try --list-themes)"))
    }
}

fn merge_icons(into: &mut IconsDef, from: IconsDef) {
    macro_rules! take {
        ($($f:ident),*) => { $( if from.$f.is_some() { into.$f = from.$f; } )* };
    }
    take!(
        app, dir, dir_open, parent, file, symlink, hidden, special, error, empty_dir, mount,
        volume, removable, network, optical, swap, unmounted, spinner
    );
    into.ext.extend(from.ext);
    into.names.extend(from.names);
}

/// Where a theme came from, for `--list-themes`.
#[derive(Debug)]
pub struct ThemeInfo {
    pub id: String,
    pub name: String,
    pub source: ThemeSource,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ThemeSource {
    BuiltIn,
    User(PathBuf),
}

/// All available themes: user themes shadow built-ins with the same id.
pub fn available_themes() -> Vec<ThemeInfo> {
    let mut list: Vec<ThemeInfo> = Vec::new();
    if let Some(dir) = crate::config::themes_dir()
        && let Ok(rd) = std::fs::read_dir(&dir)
    {
        let mut user: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect();
        user.sort();
        for p in user {
            let id = p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let name = std::fs::read_to_string(&p)
                .ok()
                .and_then(|t| toml::from_str::<ThemeDoc>(&t).ok())
                .and_then(|f| f.name)
                .unwrap_or_else(|| id.clone());
            list.push(ThemeInfo {
                id,
                name,
                source: ThemeSource::User(p),
            });
        }
    }
    for (id, text) in BUILTIN {
        if list.iter().any(|t| t.id == *id) {
            continue;
        }
        let name = toml::from_str::<ThemeDoc>(text)
            .ok()
            .and_then(|f| f.name)
            .unwrap_or_else(|| id.to_string());
        list.push(ThemeInfo {
            id: id.to_string(),
            name,
            source: ThemeSource::BuiltIn,
        });
    }
    list
}

/// Write a theme document to the user themes directory. Returns the path.
pub fn save_user_theme(id: &str, doc: &ThemeDoc) -> Result<PathBuf> {
    let dir = crate::config::themes_dir().context("no config directory on this platform")?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join(format!("{id}.toml"));
    let text = format!("# codu theme (edited in codu)\n{}", doc.to_toml());
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// Turn a display name into a file-safe theme id.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "custom".to_string()
    } else {
        out
    }
}

// ---------------------------------------------------------------------------
// Colour and style parsing

fn build_style(def: &StyleDef, palette: &BTreeMap<String, String>) -> Result<Style, String> {
    let mut style = Style::new();
    if let Some(fg) = &def.fg {
        style = style.fg(parse_color(fg, palette)?);
    }
    if let Some(bg) = &def.bg {
        style = style.bg(parse_color(bg, palette)?);
    }
    let mods = [
        (def.bold, Modifier::BOLD),
        (def.dim, Modifier::DIM),
        (def.italic, Modifier::ITALIC),
        (def.underline, Modifier::UNDERLINED),
        (def.reversed, Modifier::REVERSED),
        (def.crossed_out, Modifier::CROSSED_OUT),
    ];
    for (flag, m) in mods {
        match flag {
            Some(true) => style = style.add_modifier(m),
            Some(false) => style = style.remove_modifier(m),
            None => {}
        }
    }
    Ok(style)
}

/// Parse a colour: palette key, `#rgb`/`#rrggbb`, `rgb(r,g,b)`, ANSI index,
/// named ANSI colour, or `reset`/`none`.
pub fn parse_color(input: &str, palette: &BTreeMap<String, String>) -> Result<Color, String> {
    let raw = input.trim();
    let s = match palette.get(raw) {
        Some(v) => v.trim(),
        None => raw,
    };
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex).ok_or_else(|| format!("bad hex colour `{s}`"));
    }
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<Result<u8, _>> = inner.split(',').map(|p| p.trim().parse::<u8>()).collect();
        if let [Ok(r), Ok(g), Ok(b)] = parts[..] {
            return Ok(Color::Rgb(r, g, b));
        }
        return Err(format!("bad rgb() colour `{s}`"));
    }
    if let Ok(idx) = s.parse::<u8>() {
        return Ok(Color::Indexed(idx));
    }
    let key: String = s
        .to_ascii_lowercase()
        .chars()
        .filter(|c| !matches!(c, '_' | '-' | ' '))
        .collect();
    let key = key.replace("bright", "light");
    let c = match key.as_str() {
        "reset" | "none" | "default" => Color::Reset,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" | "purple" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "darkgrey" => Color::DarkGray,
        "lightred" => Color::LightRed,
        "lightgreen" => Color::LightGreen,
        "lightyellow" => Color::LightYellow,
        "lightblue" => Color::LightBlue,
        "lightmagenta" => Color::LightMagenta,
        "lightcyan" => Color::LightCyan,
        "lightwhite" | "lightgray" | "lightgrey" => Color::White,
        _ => return Err(format!("unknown colour `{input}`")),
    };
    Ok(c)
}

fn parse_hex(hex: &str) -> Option<Color> {
    let v = u32::from_str_radix(hex, 16).ok()?;
    match hex.len() {
        3 => {
            let r = ((v >> 8) & 0xf) as u8;
            let g = ((v >> 4) & 0xf) as u8;
            let b = (v & 0xf) as u8;
            Some(Color::Rgb(r * 17, g * 17, b * 17))
        }
        6 => Some(Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)),
        _ => None,
    }
}

/// Accept a glyph only if it renders as exactly one terminal cell.
fn single_cell(s: &str) -> Option<String> {
    let s = s.trim_end_matches('\n');
    (crate::format::width(s) == 1 && s.chars().count() == 1).then(|| s.to_string())
}

/// Linear interpolation between two colours. Falls back to a threshold pick
/// when either side is not an RGB colour.
pub fn lerp(a: Color, b: Color, t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    match (a, b) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let mix = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
            Color::Rgb(mix(r1, r2), mix(g1, g2), mix(b1, b2))
        }
        _ => {
            if t < 0.5 {
                a
            } else {
                b
            }
        }
    }
}

/// Three-stop gradient colour for a share in `0.0..=1.0`.
pub fn gradient(bar: &BarTheme, share: f64) -> Color {
    let share = if share.is_finite() {
        share.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if share < 0.5 {
        lerp(bar.low, bar.mid, share * 2.0)
    } else {
        lerp(bar.mid, bar.high, (share - 0.5) * 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_parse() {
        let pal = BTreeMap::from([("accent".to_string(), "#ff8800".to_string())]);
        assert_eq!(
            parse_color("#fff", &pal).unwrap(),
            Color::Rgb(255, 255, 255)
        );
        assert_eq!(
            parse_color("#1e1e2e", &pal).unwrap(),
            Color::Rgb(0x1e, 0x1e, 0x2e)
        );
        assert_eq!(
            parse_color("rgb(1, 2, 3)", &pal).unwrap(),
            Color::Rgb(1, 2, 3)
        );
        assert_eq!(parse_color("208", &pal).unwrap(), Color::Indexed(208));
        assert_eq!(parse_color("light_blue", &pal).unwrap(), Color::LightBlue);
        assert_eq!(parse_color("Bright-Red", &pal).unwrap(), Color::LightRed);
        assert_eq!(
            parse_color("accent", &pal).unwrap(),
            Color::Rgb(255, 136, 0)
        );
        assert_eq!(parse_color("reset", &pal).unwrap(), Color::Reset);
        assert!(parse_color("nope", &pal).is_err());
        assert!(parse_color("#12", &pal).is_err());
    }

    #[test]
    fn every_builtin_theme_parses_without_warnings() {
        let base = Theme::base();
        assert!(base.warnings.is_empty(), "{:?}", base.warnings);
        for (id, text) in BUILTIN {
            let t = Theme::parse(id, text, Some(&base)).unwrap();
            assert!(t.warnings.is_empty(), "{id}: {:?}", t.warnings);
        }
    }

    #[test]
    fn base_theme_defines_every_style_key() {
        let file: ThemeDoc = toml::from_str(BUILTIN[0].1).unwrap();
        for key in Styles::KEYS {
            assert!(file.styles.contains_key(*key), "ansi theme missing `{key}`");
        }
    }

    #[test]
    fn unknown_keys_warn_instead_of_failing() {
        let base = Theme::base();
        let t = Theme::parse(
            "x",
            "[styles]\nbogus = { fg = \"red\" }\ndir = { fg = \"nope\" }",
            Some(&base),
        )
        .unwrap();
        assert_eq!(t.warnings.len(), 2);
        assert_eq!(t.styles.dir, base.styles.dir);
    }

    #[test]
    fn partial_theme_inherits_base() {
        let base = Theme::base();
        let t = Theme::parse(
            "x",
            "name = \"X\"\n[styles]\ndir = { fg = \"#0000ff\", bold = true }",
            Some(&base),
        )
        .unwrap();
        assert_eq!(t.name, "X");
        assert_eq!(t.styles.dir.fg, Some(Color::Rgb(0, 0, 255)));
        assert_eq!(t.styles.file, base.styles.file);
        assert_eq!(t.bar.filled, base.bar.filled);
    }

    #[test]
    fn gradient_interpolates() {
        let bar = BarTheme {
            low: Color::Rgb(0, 0, 0),
            mid: Color::Rgb(100, 100, 100),
            high: Color::Rgb(200, 200, 200),
            ..BarTheme::default()
        };
        assert_eq!(gradient(&bar, 0.0), Color::Rgb(0, 0, 0));
        assert_eq!(gradient(&bar, 0.25), Color::Rgb(50, 50, 50));
        assert_eq!(gradient(&bar, 0.5), Color::Rgb(100, 100, 100));
        assert_eq!(gradient(&bar, 1.0), Color::Rgb(200, 200, 200));
        let ansi = BarTheme::default();
        assert_eq!(gradient(&ansi, 0.9), Color::Red);
    }

    #[test]
    fn doc_round_trips_through_to_toml() {
        for (id, text) in BUILTIN {
            let doc: ThemeDoc = toml::from_str(text).unwrap();
            let again: ThemeDoc = toml::from_str(&doc.to_toml())
                .unwrap_or_else(|e| panic!("{id}: {e}\n{}", doc.to_toml()));
            assert_eq!(doc.styles, again.styles, "{id}");
            assert_eq!(doc.palette, again.palette, "{id}");
            assert_eq!(doc.name, again.name);
            let base = Theme::base();
            let a = Theme::from_doc(id, &doc, Some(&base));
            let b = Theme::from_doc(id, &again, Some(&base));
            for k in Styles::KEYS {
                assert_eq!(a.styles.get(k), b.styles.get(k), "{id} {k}");
            }
        }
    }

    #[test]
    fn inline_style_and_keys() {
        let d = StyleDef {
            fg: Some("#123456".into()),
            bold: Some(true),
            ..StyleDef::default()
        };
        assert_eq!(d.to_inline(), "{ fg = \"#123456\", bold = true }");
        assert_eq!(StyleDef::default().to_inline(), "{}");
        assert_eq!(key("dir_open"), "dir_open");
        assert_eq!(key("docker-compose.yml"), "\"docker-compose.yml\"");
    }

    #[test]
    fn slugify_names() {
        assert_eq!(slugify("My Theme!"), "my-theme");
        assert_eq!(slugify("  Nord (light) "), "nord-light");
        assert_eq!(slugify("???"), "custom");
    }

    #[test]
    fn bar_glyphs_must_be_single_cell() {
        let base = Theme::base();
        let t = Theme::parse("x", "[bar]\nfilled = \"📁\"\nempty = \"-\"", Some(&base)).unwrap();
        assert_eq!(t.warnings.len(), 1);
        assert_eq!(t.bar.empty, "-");
        assert_eq!(t.bar.filled, base.bar.filled);
    }
}
