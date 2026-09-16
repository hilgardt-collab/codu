//! In-app theme editor state. Pure state transitions over a [`ThemeDoc`];
//! the app applies the resolved theme after each change.

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
            EditorRow::Style(_) => self.open_prompt(Field::Fg),
            EditorRow::BarFilled => self.open_prompt(Field::BarFilled),
            EditorRow::BarEmpty => self.open_prompt(Field::BarEmpty),
            EditorRow::BarGradient => self.toggle_gradient(),
            EditorRow::BarLow => self.open_prompt(Field::BarLow),
            EditorRow::BarMid => self.open_prompt(Field::BarMid),
            EditorRow::BarHigh => self.open_prompt(Field::BarHigh),
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
        match (p.field, self.row()) {
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
        self.prompt = None;
        self.mark_dirty();
        true
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
