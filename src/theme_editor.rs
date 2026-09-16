//! In-app theme editor state. Pure state transitions over a [`ThemeDoc`];
//! the app applies the resolved theme after each change.

use ratatui::style::Color;

use crate::theme::{StyleDef, Styles, Theme, ThemeDoc, parse_color};

/// What a row in the editor edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorRow {
    Name,
    Dark,
    Style(&'static str),
    BarFilled,
    BarEmpty,
    BarGradient,
    BarLow,
    BarMid,
    BarHigh,
}

impl EditorRow {
    pub fn label(&self) -> String {
        match self {
            EditorRow::Name => "name".into(),
            EditorRow::Dark => "dark".into(),
            EditorRow::Style(k) => (*k).to_string(),
            EditorRow::BarFilled => "bar.filled".into(),
            EditorRow::BarEmpty => "bar.empty".into(),
            EditorRow::BarGradient => "bar.gradient".into(),
            EditorRow::BarLow => "bar.low".into(),
            EditorRow::BarMid => "bar.mid".into(),
            EditorRow::BarHigh => "bar.high".into(),
        }
    }
}

/// Which value a text prompt is editing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    Fg,
    Bg,
    BarFilled,
    BarEmpty,
    BarLow,
    BarMid,
    BarHigh,
}

#[derive(Clone, Debug)]
pub struct Prompt {
    pub field: Field,
    /// What the user has typed so far (starts empty).
    pub text: String,
    /// The value in effect, shown as a hint.
    pub current: String,
}

#[derive(Clone, Debug)]
pub struct ThemeEditor {
    pub id: String,
    pub doc: ThemeDoc,
    pub rows: Vec<EditorRow>,
    pub cursor: usize,
    pub prompt: Option<Prompt>,
    /// Slider-based colour picker, open for one colour field at a time.
    pub picker: Option<ColorPicker>,
    pub dirty: bool,
    /// Feedback line (parse errors, save confirmation).
    pub message: Option<(String, bool)>,
    /// Theme in effect before editing began, restored on discard.
    pub original: Theme,
    /// Set after Esc with unsaved changes; a second Esc discards.
    pub confirm_discard: bool,
}

impl ThemeEditor {
    pub fn new(id: String, doc: ThemeDoc, original: Theme) -> Self {
        let mut rows = vec![EditorRow::Name, EditorRow::Dark];
        rows.extend(Styles::KEYS.iter().map(|k| EditorRow::Style(k)));
        rows.extend([
            EditorRow::BarFilled,
            EditorRow::BarEmpty,
            EditorRow::BarGradient,
            EditorRow::BarLow,
            EditorRow::BarMid,
            EditorRow::BarHigh,
        ]);
        ThemeEditor {
            id,
            doc,
            rows,
            cursor: 2,
            prompt: None,
            picker: None,
            dirty: false,
            message: None,
            original,
            confirm_discard: false,
        }
    }

    pub fn row(&self) -> EditorRow {
        self.rows[self.cursor]
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let n = self.rows.len() as isize;
        self.cursor = (self.cursor as isize + delta).clamp(0, n - 1) as usize;
        self.confirm_discard = false;
    }

    fn style_mut(&mut self, key: &str) -> &mut StyleDef {
        self.doc.styles.entry(key.to_string()).or_default()
    }

    /// Current raw definition of a style row (empty when inherited).
    pub fn style_def(&self, key: &str) -> StyleDef {
        self.doc.styles.get(key).cloned().unwrap_or_default()
    }

    /// Text the prompt starts with for a field on the current row.
    fn current_text(&self, field: Field) -> String {
        match (field, self.row()) {
            (Field::Name, _) => self.doc.name.clone().unwrap_or_else(|| self.id.clone()),
            (Field::Fg, EditorRow::Style(k)) => self.style_def(k).fg.unwrap_or_default(),
            (Field::Bg, EditorRow::Style(k)) => self.style_def(k).bg.unwrap_or_default(),
            (Field::BarFilled, _) => self.doc.bar.filled.clone().unwrap_or_default(),
            (Field::BarEmpty, _) => self.doc.bar.empty.clone().unwrap_or_default(),
            (Field::BarLow, _) => self.doc.bar.low.clone().unwrap_or_default(),
            (Field::BarMid, _) => self.doc.bar.mid.clone().unwrap_or_default(),
            (Field::BarHigh, _) => self.doc.bar.high.clone().unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// Open a text prompt for `field` if it applies to the current row.
    pub fn open_prompt(&mut self, field: Field) {
        let applies = matches!(
            (field, self.row()),
            (Field::Name, _)
                | (Field::Fg | Field::Bg, EditorRow::Style(_))
                | (Field::BarFilled, EditorRow::BarFilled)
                | (Field::BarEmpty, EditorRow::BarEmpty)
                | (Field::BarLow, EditorRow::BarLow)
                | (Field::BarMid, EditorRow::BarMid)
                | (Field::BarHigh, EditorRow::BarHigh)
        );
        if applies {
            self.prompt = Some(Prompt {
                field,
                text: String::new(),
                current: self.current_text(field),
            });
            self.message = None;
        }
    }

    /// Enter on a row: the most natural edit for that row.
    pub fn activate(&mut self) {
        match self.row() {
            EditorRow::Name => self.open_prompt(Field::Name),
            EditorRow::Dark => self.toggle_dark(),
            EditorRow::Style(_) => self.open_picker(Field::Fg),
            EditorRow::BarFilled => self.open_prompt(Field::BarFilled),
            EditorRow::BarEmpty => self.open_prompt(Field::BarEmpty),
            EditorRow::BarGradient => self.toggle_gradient(),
            EditorRow::BarLow => self.open_picker(Field::BarLow),
            EditorRow::BarMid => self.open_picker(Field::BarMid),
            EditorRow::BarHigh => self.open_picker(Field::BarHigh),
        }
    }

    /// Apply the prompt's text: empty keeps the current value, `none` or `-`
    /// makes the row inherit from the base theme. Returns false (keeping the
    /// prompt open) on a value that does not validate.
    pub fn commit_prompt(&mut self) -> bool {
        let Some(p) = self.prompt.clone() else {
            return true;
        };
        let text = p.text.trim().to_string();
        if text.is_empty() {
            self.prompt = None;
            return true;
        }
        let value = if text == "none" || text == "-" {
            None
        } else {
            Some(text.clone())
        };
        let is_colour = matches!(
            p.field,
            Field::Fg | Field::Bg | Field::BarLow | Field::BarMid | Field::BarHigh
        );
        if is_colour
            && let Some(v) = &value
            && let Err(e) = parse_color(v, &self.doc.palette)
        {
            self.message = Some((e, true));
            return false;
        }
        if matches!(p.field, Field::BarFilled | Field::BarEmpty)
            && let Some(v) = &value
            && (crate::format::width(v) != 1 || v.chars().count() != 1)
        {
            self.message = Some(("bar glyphs must be exactly one cell wide".into(), true));
            return false;
        }
        self.set_field(p.field, value);
        self.prompt = None;
        self.mark_dirty();
        true
    }

    /// Write a raw value into the field on the current row (`None` = inherit).
    fn set_field(&mut self, field: Field, value: Option<String>) {
        match (field, self.row()) {
            (Field::Name, _) => self.doc.name = value,
            (Field::Fg, EditorRow::Style(k)) => {
                self.style_mut(k).fg = value;
                self.prune(k);
            }
            (Field::Bg, EditorRow::Style(k)) => {
                self.style_mut(k).bg = value;
                self.prune(k);
            }
            (Field::BarFilled, _) => self.doc.bar.filled = value,
            (Field::BarEmpty, _) => self.doc.bar.empty = value,
            (Field::BarLow, _) => self.doc.bar.low = value,
            (Field::BarMid, _) => self.doc.bar.mid = value,
            (Field::BarHigh, _) => self.doc.bar.high = value,
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Colour picker

    /// Open the slider picker for a colour field on the current row.
    pub fn open_picker(&mut self, field: Field) {
        let applies = matches!(
            (field, self.row()),
            (Field::Fg | Field::Bg, EditorRow::Style(_))
                | (Field::BarLow, EditorRow::BarLow)
                | (Field::BarMid, EditorRow::BarMid)
                | (Field::BarHigh, EditorRow::BarHigh)
        );
        if !applies {
            return;
        }
        let current = self.current_text(field);
        let original = if current.is_empty() {
            None
        } else {
            Some(current.clone())
        };
        let resolved = if current.is_empty() {
            None
        } else {
            parse_color(&current, &self.doc.palette).ok()
        };
        let rgb = resolved
            .and_then(color_to_rgb)
            .unwrap_or(if field == Field::Bg {
                [30, 30, 46]
            } else {
                [200, 200, 200]
            });
        let literal = match (
            &resolved,
            current.starts_with('#') || current.starts_with("rgb("),
        ) {
            (Some(_), false) => Some(current.clone()),
            _ => None,
        };
        let what = match field {
            Field::Fg => "foreground",
            Field::Bg => "background",
            Field::BarLow => "gradient low",
            Field::BarMid => "gradient mid",
            Field::BarHigh => "gradient high",
            _ => "colour",
        };
        self.picker = Some(ColorPicker {
            field,
            rgb,
            hsv: rgb_to_hsv(rgb),
            channel: 0,
            literal,
            entry: None,
            original,
            dirty_before: self.dirty,
            label: format!("{} {what}", self.row().label()),
        });
        self.prompt = None;
        self.message = None;
    }

    /// Push the picker's current value into the document (live preview).
    pub fn picker_store(&mut self) {
        let Some(p) = &self.picker else { return };
        let (field, value) = (p.field, Some(p.value_string()));
        self.set_field(field, value);
        self.dirty = true;
        self.confirm_discard = false;
    }

    /// Keep the picker's value and close it.
    pub fn picker_apply(&mut self) {
        self.picker_store();
        self.picker = None;
    }

    /// Restore the value from before the picker opened and close it.
    pub fn picker_cancel(&mut self) {
        let Some(p) = self.picker.take() else { return };
        self.set_field(p.field, p.original);
        self.dirty = p.dirty_before;
        self.message = None;
    }

    /// Apply the picker's typed entry: a hex/rgb() value moves the sliders,
    /// a palette key or ANSI name is kept literally. Returns false on error.
    pub fn picker_entry_commit(&mut self) -> bool {
        let palette = self.doc.palette.clone();
        let Some(p) = self.picker.as_mut() else {
            return true;
        };
        let Some(text) = p.entry.take() else {
            return true;
        };
        let text = text.trim().to_string();
        if text.is_empty() {
            return true;
        }
        match parse_color(&text, &palette) {
            Ok(c) => {
                if text.starts_with('#') || text.starts_with("rgb(") {
                    if let Some(rgb) = color_to_rgb(c) {
                        p.set_rgb(rgb);
                    }
                } else {
                    if let Some(rgb) = color_to_rgb(c) {
                        p.set_rgb(rgb);
                    }
                    p.literal = Some(text);
                }
                self.message = None;
                self.picker_store();
                true
            }
            Err(e) => {
                p.entry = Some(text);
                self.message = Some((e, true));
                false
            }
        }
    }

    pub fn cancel_prompt(&mut self) {
        self.prompt = None;
        self.message = None;
    }

    /// Flip a boolean attribute on the current style row.
    pub fn toggle_attr(&mut self, attr: Attr) {
        let EditorRow::Style(k) = self.row() else {
            return;
        };
        let def = self.style_mut(k);
        let slot = match attr {
            Attr::Bold => &mut def.bold,
            Attr::Dim => &mut def.dim,
            Attr::Italic => &mut def.italic,
            Attr::Underline => &mut def.underline,
            Attr::Reversed => &mut def.reversed,
            Attr::CrossedOut => &mut def.crossed_out,
        };
        *slot = match *slot {
            Some(true) => None,
            _ => Some(true),
        };
        self.prune(k);
        self.mark_dirty();
    }

    /// A style with nothing set is removed so the row inherits from the base
    /// theme again (a present-but-empty key would override it with defaults).
    fn prune(&mut self, key: &str) {
        if self
            .doc
            .styles
            .get(key)
            .is_some_and(|d| *d == StyleDef::default())
        {
            self.doc.styles.remove(key);
        }
    }

    /// Whether the row's style is defined by this theme (vs. inherited).
    pub fn defines(&self, key: &str) -> bool {
        self.doc.styles.contains_key(key)
    }

    /// Remove the current row's definition so it inherits from the base.
    pub fn clear_row(&mut self) {
        match self.row() {
            EditorRow::Name => self.doc.name = None,
            EditorRow::Dark => self.doc.dark = None,
            EditorRow::Style(k) => {
                self.doc.styles.remove(k);
            }
            EditorRow::BarFilled => self.doc.bar.filled = None,
            EditorRow::BarEmpty => self.doc.bar.empty = None,
            EditorRow::BarGradient => self.doc.bar.gradient = None,
            EditorRow::BarLow => self.doc.bar.low = None,
            EditorRow::BarMid => self.doc.bar.mid = None,
            EditorRow::BarHigh => self.doc.bar.high = None,
        }
        self.mark_dirty();
    }

    pub fn toggle_dark(&mut self) {
        self.doc.dark = Some(!self.doc.dark.unwrap_or(true));
        self.mark_dirty();
    }

    pub fn toggle_gradient(&mut self) {
        self.doc.bar.gradient = Some(!self.doc.bar.gradient.unwrap_or(false));
        self.mark_dirty();
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.confirm_discard = false;
        self.message = None;
    }

    /// Resolve the document into a live theme (base = ansi).
    pub fn resolved(&self) -> Theme {
        Theme::from_doc(&self.id, &self.doc, Some(&Theme::base()))
    }
}

/// Slider-based colour editor for one colour field.
#[derive(Clone, Debug)]
pub struct ColorPicker {
    pub field: Field,
    pub rgb: [u8; 3],
    /// Hue 0..360, saturation 0..1, value 0..1.
    pub hsv: [f64; 3],
    /// Active slider: 0 R, 1 G, 2 B, 3 H, 4 S, 5 V.
    pub channel: usize,
    /// A palette key or ANSI name chosen by typing; cleared by slider edits.
    pub literal: Option<String>,
    /// Inline text entry for a hex value or a name.
    pub entry: Option<String>,
    /// Field value when the picker opened, restored on cancel.
    pub original: Option<String>,
    pub dirty_before: bool,
    /// "dir foreground", for the title.
    pub label: String,
}

pub const CHANNELS: [&str; 6] = ["R", "G", "B", "H", "S", "V"];

impl ColorPicker {
    pub fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.rgb[0], self.rgb[1], self.rgb[2])
    }

    /// What gets written to the theme: the typed name, or the hex value.
    pub fn value_string(&self) -> String {
        self.literal.clone().unwrap_or_else(|| self.hex())
    }

    pub fn set_rgb(&mut self, rgb: [u8; 3]) {
        self.rgb = rgb;
        self.hsv = rgb_to_hsv(rgb);
        self.literal = None;
    }

    pub fn set_hsv(&mut self, hsv: [f64; 3]) {
        let h = hsv[0].rem_euclid(360.0);
        self.hsv = [h, hsv[1].clamp(0.0, 1.0), hsv[2].clamp(0.0, 1.0)];
        self.rgb = hsv_to_rgb(self.hsv);
        self.literal = None;
    }

    /// Current value of the active channel in its display units
    /// (0..255 for RGB, degrees for H, percent for S and V).
    pub fn channel_value(&self, ch: usize) -> i32 {
        match ch {
            0..=2 => self.rgb[ch] as i32,
            3 => self.hsv[0].round() as i32,
            4 => (self.hsv[1] * 100.0).round() as i32,
            _ => (self.hsv[2] * 100.0).round() as i32,
        }
    }

    /// Move the active channel by `delta` display units.
    pub fn adjust(&mut self, delta: i32) {
        match self.channel {
            ch @ 0..=2 => {
                let mut rgb = self.rgb;
                rgb[ch] = (rgb[ch] as i32 + delta).clamp(0, 255) as u8;
                self.set_rgb(rgb);
            }
            3 => {
                let mut hsv = self.hsv;
                hsv[0] += delta as f64;
                self.set_hsv(hsv);
            }
            4 => {
                let mut hsv = self.hsv;
                hsv[1] += delta as f64 / 100.0;
                self.set_hsv(hsv);
            }
            _ => {
                let mut hsv = self.hsv;
                hsv[2] += delta as f64 / 100.0;
                self.set_hsv(hsv);
            }
        }
    }

    /// Jump the active channel to its minimum or maximum.
    pub fn set_extreme(&mut self, max: bool) {
        match self.channel {
            ch @ 0..=2 => {
                let mut rgb = self.rgb;
                rgb[ch] = if max { 255 } else { 0 };
                self.set_rgb(rgb);
            }
            3 => {
                let mut hsv = self.hsv;
                hsv[0] = if max { 359.0 } else { 0.0 };
                self.set_hsv(hsv);
            }
            ch => {
                let mut hsv = self.hsv;
                hsv[ch - 3] = if max { 1.0 } else { 0.0 };
                self.set_hsv(hsv);
            }
        }
    }

    /// Colour the slider would produce at position `t` in 0..=1.
    pub fn color_at(&self, ch: usize, t: f64) -> Color {
        let t = t.clamp(0.0, 1.0);
        let [r, g, b] = self.rgb;
        let [h, s, v] = self.hsv;
        let rgb = match ch {
            0 => [(t * 255.0) as u8, g, b],
            1 => [r, (t * 255.0) as u8, b],
            2 => [r, g, (t * 255.0) as u8],
            3 => hsv_to_rgb([t * 359.0, s.max(0.35), v.max(0.5)]),
            4 => hsv_to_rgb([h, t, v.max(0.3)]),
            _ => hsv_to_rgb([h, s, t]),
        };
        Color::Rgb(rgb[0], rgb[1], rgb[2])
    }
}

pub fn rgb_to_hsv(rgb: [u8; 3]) -> [f64; 3] {
    let r = rgb[0] as f64 / 255.0;
    let g = rgb[1] as f64 / 255.0;
    let b = rgb[2] as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    [h, s, max]
}

pub fn hsv_to_rgb(hsv: [f64; 3]) -> [u8; 3] {
    let h = hsv[0].rem_euclid(360.0);
    let s = hsv[1].clamp(0.0, 1.0);
    let v = hsv[2].clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to = |f: f64| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [to(r), to(g), to(b)]
}

/// Approximate RGB for the 16 named ANSI colours (xterm defaults).
pub const ANSI_NAMED: [(&str, Color, [u8; 3]); 16] = [
    ("black", Color::Black, [0, 0, 0]),
    ("red", Color::Red, [205, 0, 0]),
    ("green", Color::Green, [0, 205, 0]),
    ("yellow", Color::Yellow, [205, 205, 0]),
    ("blue", Color::Blue, [0, 0, 238]),
    ("magenta", Color::Magenta, [205, 0, 205]),
    ("cyan", Color::Cyan, [0, 205, 205]),
    ("gray", Color::Gray, [229, 229, 229]),
    ("darkgray", Color::DarkGray, [127, 127, 127]),
    ("lightred", Color::LightRed, [255, 0, 0]),
    ("lightgreen", Color::LightGreen, [0, 255, 0]),
    ("lightyellow", Color::LightYellow, [255, 255, 0]),
    ("lightblue", Color::LightBlue, [92, 92, 255]),
    ("lightmagenta", Color::LightMagenta, [255, 0, 255]),
    ("lightcyan", Color::LightCyan, [0, 255, 255]),
    ("white", Color::White, [255, 255, 255]),
];

/// RGB of an xterm-256 index.
pub fn xterm256_rgb(idx: u8) -> [u8; 3] {
    match idx {
        0..=15 => ANSI_NAMED[idx as usize].2,
        16..=231 => {
            let i = idx - 16;
            let step = |n: u8| if n == 0 { 0 } else { 55 + n * 40 };
            [step(i / 36), step((i / 6) % 6), step(i % 6)]
        }
        _ => {
            let g = 8 + (idx - 232) * 10;
            [g, g, g]
        }
    }
}

/// Best-effort RGB for any ratatui colour.
pub fn color_to_rgb(c: Color) -> Option<[u8; 3]> {
    match c {
        Color::Rgb(r, g, b) => Some([r, g, b]),
        Color::Indexed(i) => Some(xterm256_rgb(i)),
        Color::Reset => None,
        other => ANSI_NAMED
            .iter()
            .find(|(_, col, _)| *col == other)
            .map(|(_, _, rgb)| *rgb),
    }
}

fn distance(a: [u8; 3], b: [u8; 3]) -> u32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (*x as i32 - y as i32).pow(2) as u32)
        .sum()
}

/// Closest of the 16 named ANSI colours.
pub fn nearest_ansi(rgb: [u8; 3]) -> &'static str {
    ANSI_NAMED
        .iter()
        .min_by_key(|(_, _, c)| distance(rgb, *c))
        .map(|(n, _, _)| *n)
        .unwrap_or("white")
}

/// Closest xterm-256 index.
pub fn nearest_xterm256(rgb: [u8; 3]) -> u8 {
    (0..=255u8)
        .min_by_key(|&i| distance(rgb, xterm256_rgb(i)))
        .unwrap_or(15)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attr {
    Bold,
    Dim,
    Italic,
    Underline,
    Reversed,
    CrossedOut,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Modifier};

    fn editor() -> ThemeEditor {
        let (id, doc) = Theme::load_doc("nord").unwrap();
        let t = Theme::from_doc(&id, &doc, Some(&Theme::base()));
        ThemeEditor::new(id, doc, t)
    }

    fn goto(e: &mut ThemeEditor, key: &str) {
        e.cursor = e
            .rows
            .iter()
            .position(|r| matches!(r, EditorRow::Style(k) if *k == key))
            .unwrap();
    }

    #[test]
    fn edits_fg_with_validation() {
        let mut e = editor();
        goto(&mut e, "dir");
        e.open_prompt(Field::Fg);
        assert_eq!(e.prompt.as_ref().unwrap().current, "nord9");
        assert!(e.prompt.as_ref().unwrap().text.is_empty());
        // Empty keeps the value and closes the prompt.
        assert!(e.commit_prompt());
        assert!(e.prompt.is_none());
        assert!(!e.dirty);
        e.open_prompt(Field::Fg);
        e.prompt.as_mut().unwrap().text = "not-a-colour".into();
        assert!(!e.commit_prompt());
        assert!(e.prompt.is_some());
        assert!(e.message.as_ref().unwrap().1);
        e.prompt.as_mut().unwrap().text = "#ff0000".into();
        assert!(e.commit_prompt());
        assert!(e.dirty);
        assert_eq!(e.resolved().styles.dir.fg, Some(Color::Rgb(255, 0, 0)));
        // Palette keys still work.
        e.open_prompt(Field::Bg);
        e.prompt.as_mut().unwrap().text = "nord1".into();
        assert!(e.commit_prompt());
        assert_eq!(
            e.resolved().styles.dir.bg,
            Some(Color::Rgb(0x3b, 0x42, 0x52))
        );
        // "none" clears the field; with bg still set the key stays defined
        // and fg falls back to the terminal default.
        e.open_prompt(Field::Fg);
        e.prompt.as_mut().unwrap().text = "none".into();
        assert!(e.commit_prompt());
        assert_eq!(e.style_def("dir").fg, None);
        assert!(e.defines("dir"));
        assert_eq!(e.resolved().styles.dir.fg, None);
        // Clearing the last field removes the key, so it inherits the base.
        e.open_prompt(Field::Bg);
        e.prompt.as_mut().unwrap().text = "-".into();
        assert!(e.commit_prompt());
        assert!(e.defines("dir"), "bold is still set");
        e.toggle_attr(Attr::Bold);
        assert!(!e.defines("dir"));
        assert_eq!(e.resolved().styles.dir, Theme::base().styles.dir);
    }

    #[test]
    fn toggles_and_clear() {
        let mut e = editor();
        goto(&mut e, "file");
        e.toggle_attr(Attr::Bold);
        assert!(
            e.resolved()
                .styles
                .file
                .add_modifier
                .contains(Modifier::BOLD)
        );
        e.toggle_attr(Attr::Bold);
        assert!(
            !e.resolved()
                .styles
                .file
                .add_modifier
                .contains(Modifier::BOLD)
        );
        e.clear_row();
        assert!(!e.doc.styles.contains_key("file"));
        assert_eq!(e.resolved().styles.file, Theme::base().styles.file);
    }

    #[test]
    fn hsv_round_trips() {
        for rgb in [
            [0, 0, 0],
            [255, 255, 255],
            [137, 180, 250],
            [205, 0, 0],
            [12, 200, 90],
            [128, 128, 128],
        ] {
            let back = hsv_to_rgb(rgb_to_hsv(rgb));
            for i in 0..3 {
                assert!(
                    (back[i] as i32 - rgb[i] as i32).abs() <= 1,
                    "{rgb:?} -> {back:?}"
                );
            }
        }
        assert_eq!(rgb_to_hsv([255, 0, 0])[0], 0.0);
        assert_eq!(rgb_to_hsv([0, 255, 0])[0], 120.0);
        assert_eq!(hsv_to_rgb([240.0, 1.0, 1.0]), [0, 0, 255]);
    }

    #[test]
    fn nearest_colours_and_xterm() {
        assert_eq!(nearest_ansi([250, 5, 5]), "lightred");
        assert_eq!(nearest_ansi([0, 0, 0]), "black");
        assert_eq!(xterm256_rgb(16), [0, 0, 0]);
        assert_eq!(xterm256_rgb(231), [255, 255, 255]);
        assert_eq!(xterm256_rgb(196), [255, 0, 0]);
        assert_eq!(nearest_xterm256([255, 0, 0]), 9); // lightred beats 196 on exact match order
        assert_eq!(nearest_xterm256([95, 135, 175]), 67);
        assert_eq!(color_to_rgb(Color::Indexed(196)), Some([255, 0, 0]));
        assert_eq!(color_to_rgb(Color::Reset), None);
    }

    #[test]
    fn picker_edits_live_and_cancel_restores() {
        let mut e = editor();
        goto(&mut e, "dir");
        e.open_picker(Field::Fg);
        let p = e.picker.as_ref().unwrap();
        assert_eq!(p.literal.as_deref(), Some("nord9"));
        assert_eq!(p.rgb, [0x81, 0xa1, 0xc1]);
        assert_eq!(p.original.as_deref(), Some("nord9"));
        // Slider edits become hex and preview live.
        e.picker.as_mut().unwrap().channel = 0;
        e.picker.as_mut().unwrap().adjust(10);
        e.picker_store();
        assert_eq!(e.style_def("dir").fg.as_deref(), Some("#8ba1c1"));
        assert!(e.dirty);
        assert_eq!(
            e.resolved().styles.dir.fg,
            Some(Color::Rgb(0x8b, 0xa1, 0xc1))
        );
        // HSV edits keep RGB in sync.
        e.picker.as_mut().unwrap().channel = 5;
        e.picker.as_mut().unwrap().set_extreme(false);
        assert_eq!(e.picker.as_ref().unwrap().rgb, [0, 0, 0]);
        e.picker.as_mut().unwrap().channel = 3;
        e.picker.as_mut().unwrap().adjust(-30);
        assert!(e.picker.as_ref().unwrap().hsv[0] >= 0.0);
        // Cancel restores the palette key and the dirty flag.
        e.picker_cancel();
        assert!(e.picker.is_none());
        assert_eq!(e.style_def("dir").fg.as_deref(), Some("nord9"));
        assert!(!e.dirty);
    }

    #[test]
    fn picker_entry_accepts_hex_and_palette_keys() {
        let mut e = editor();
        goto(&mut e, "file");
        e.open_picker(Field::Bg);
        assert_eq!(e.picker.as_ref().unwrap().original, None);
        e.picker.as_mut().unwrap().entry = Some("#ff0000".into());
        assert!(e.picker_entry_commit());
        assert_eq!(e.picker.as_ref().unwrap().rgb, [255, 0, 0]);
        assert_eq!(e.style_def("file").bg.as_deref(), Some("#ff0000"));
        e.picker.as_mut().unwrap().entry = Some("nord3".into());
        assert!(e.picker_entry_commit());
        assert_eq!(e.picker.as_ref().unwrap().value_string(), "nord3");
        assert_eq!(e.style_def("file").bg.as_deref(), Some("nord3"));
        e.picker.as_mut().unwrap().entry = Some("bogus".into());
        assert!(!e.picker_entry_commit());
        assert!(e.message.as_ref().unwrap().1);
        e.picker_apply();
        assert!(e.picker.is_none());
        assert_eq!(e.style_def("file").bg.as_deref(), Some("nord3"));
        // Cancel after apply does nothing; original was None so a fresh
        // open + cancel removes the field again.
        e.open_picker(Field::Bg);
        e.picker_cancel();
        assert_eq!(e.style_def("file").bg.as_deref(), Some("nord3"));
    }

    #[test]
    fn bar_glyph_validation_and_name() {
        let mut e = editor();
        e.cursor = e
            .rows
            .iter()
            .position(|r| *r == EditorRow::BarFilled)
            .unwrap();
        e.activate();
        e.prompt.as_mut().unwrap().text = "📁".into();
        assert!(!e.commit_prompt());
        e.prompt.as_mut().unwrap().text = "#".into();
        assert!(e.commit_prompt());
        assert_eq!(e.resolved().bar.filled, "#");
        e.cursor = 0;
        e.activate();
        e.prompt.as_mut().unwrap().text = "My Nord".into();
        assert!(e.commit_prompt());
        assert_eq!(e.resolved().name, "My Nord");
    }
}
