//! Modal popups: help, info, delete confirmation, theme picker, messages.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Wrap};

use crate::app::{App, DeleteMode, Popup};
use crate::format;
use crate::scan::Kind;
use crate::ui::{centered, dim};

pub fn draw(frame: &mut Frame, app: &App) {
    if matches!(app.popup, Popup::None) {
        return;
    }
    let area = frame.area();
    dim(frame.buffer_mut(), area);
    match &app.popup {
        Popup::None => {}
        Popup::Help { scroll } => help(frame, app, *scroll),
        Popup::Info => info(frame, app),
        Popup::Confirm { mode, path } => confirm(frame, app, *mode, path),
        Popup::Themes { cursor, prompt, .. } => themes(frame, app, *cursor, prompt.as_deref()),
        Popup::ThemeEditor(ed) => editor(frame, app, ed),
        Popup::Options => options(frame, app),
        Popup::Message {
            title,
            body,
            danger,
        } => message(frame, app, title, body, *danger),
    }
}

/// Draw a popup frame and return its inner area.
fn frame_box(
    frame: &mut Frame,
    app: &App,
    title: &str,
    width: u16,
    height: u16,
    danger: bool,
) -> Rect {
    let st = &app.theme.styles;
    let rect = centered(frame.area(), width, height);
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(if danger { st.danger } else { st.popup_border })
        .style(st.popup)
        .title(Line::from(format!(" {title} ")).style(if danger {
            st.danger
        } else {
            st.popup_title
        }));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}

fn icon_or<'a>(app: &App, emoji: &'a str) -> &'a str {
    if app.icons.emoji() { emoji } else { "" }
}

const HELP: &[(&str, &str)] = &[
    ("", "Navigate"),
    ("↑ k / ↓ j", "move selection"),
    ("PgUp PgDn", "page up / down"),
    ("Home g / End G", "first / last entry"),
    ("Tab  v", "switch between list and tree view"),
    ("/", "filter listing (Esc clears)"),
    ("", ""),
    ("", "List view"),
    (
        "→ ⏎ l",
        "open directory (on .. at the top: scan the parent)",
    ),
    ("← ⌫ h", "parent directory"),
    ("", ""),
    ("", "Tree view"),
    ("+ = → l", "expand (→ again steps into the first child)"),
    ("- ← h", "collapse (← again jumps to the parent)"),
    ("Space ⏎", "toggle expand/collapse"),
    ("*", "expand everything below"),
    ("⌫", "jump to the parent row"),
    ("", ""),
    ("", "Sort & view"),
    (
        "s n C M",
        "sort: size / name / items / mtime (again reverses)",
    ),
    ("t", "toggle directories first"),
    ("y", "toggle group by type (captioned sections)"),
    ("o", "options popup: every toggle with its current state"),
    ("a", "toggle disk usage / apparent size"),
    ("b", "cycle bar: bar+percent, bar, percent, none"),
    ("c  m", "toggle item-count / mtime column"),
    ("e", "toggle hidden entries"),
    ("T", "theme picker: e edit, n new copy, S set default"),
    ("V", "volumes screen (also: .. at the filesystem root)"),
    ("", ""),
    ("", "Volumes screen"),
    ("⏎ →", "scan the selected mounted volume"),
    ("r", "refresh the list"),
    (
        "Esc ← V",
        "back to the browser (Esc quits if nothing is scanned)",
    ),
    ("", ""),
    ("", "Actions"),
    ("i", "info about the selected entry"),
    ("d", "delete permanently (asks first)"),
    ("D", "move to trash (asks first)"),
    ("r", "rescan (list: current directory, tree: selected)"),
    ("?  F1", "this help"),
    (
        "Esc  Ctrl-C",
        "quit (Esc first closes popups / clears the filter)",
    ),
    ("", ""),
    ("", "Flags"),
    ("!", "directory could not be read"),
    (".", "a subdirectory could not be read"),
    ("<", "excluded by pattern"),
    (">", "on another filesystem"),
    ("@", "symlink or special file"),
    ("H", "hard link, counted elsewhere"),
    ("e", "empty directory"),
    ("", ""),
    (
        "",
        "Mouse: click selects, double-click opens/toggles, wheel scrolls.",
    ),
];

fn help(frame: &mut Frame, app: &App, scroll: u16) {
    let st = &app.theme.styles;
    let area = frame.area();
    let height = (HELP.len() as u16 + 2).min(area.height.saturating_sub(2));
    let inner = frame_box(
        frame,
        app,
        &format!(
            "{}Help  (↑↓ scroll · any other key closes)",
            icon_or(app, "❓ ")
        ),
        70,
        height,
        false,
    );

    let lines: Vec<Line> = HELP
        .iter()
        .map(|(key, desc)| {
            if key.is_empty() {
                Line::from(Span::styled(format!(" {desc}"), st.popup_title))
            } else {
                Line::from(vec![
                    Span::styled(format!("  {}", format::fit(key, 14)), st.help_key),
                    Span::styled(*desc, st.help_desc),
                ])
            }
        })
        .collect();
    let max_scroll = (lines.len() as u16).saturating_sub(inner.height);
    let para = Paragraph::new(Text::from(lines)).scroll((scroll.min(max_scroll), 0));
    frame.render_widget(para, inner);
}

fn info(frame: &mut Frame, app: &App) {
    let st = &app.theme.styles;
    let Some(node) = app.selected_node() else {
        return;
    };
    let Some(tree_path) = app.selected_path() else {
        return;
    };
    let path = app.fs_path(&tree_path);
    let base = app.selected_share_base();
    let share = if base > 0 {
        node.size_of(app.apparent) as f64 / base as f64
    } else {
        0.0
    };
    let parent_label = match tree_path.split_last() {
        Some((_, p)) if !p.is_empty() => {
            format::truncate_left(&app.fs_path(p).display().to_string(), 40)
        }
        Some(_) => "root".to_string(),
        None => "itself".to_string(),
    };

    let (subdirs, files) = if node.is_dir() {
        let d = node.children.iter().filter(|c| c.is_dir()).count();
        (d, node.children.len() - d)
    } else {
        (0, 0)
    };

    let mut rows: Vec<(&str, String)> = vec![
        ("Name", node.name.to_string()),
        ("Path", path.display().to_string()),
        ("Type", node.describe()),
        (
            "Disk usage",
            format!(
                "{}  ({} bytes)",
                format::bytes_str(node.size, app.si),
                format::count(node.size)
            ),
        ),
        (
            "Apparent",
            format!(
                "{}  ({} bytes)",
                format::bytes_str(node.apparent, app.si),
                format::count(node.apparent)
            ),
        ),
        (
            "Share",
            format!("{} of {}", format::percent(share), parent_label),
        ),
    ];
    if node.has(crate::scan::flags::OTHER_FS) {
        match app.volume_for(&path) {
            Some(v) => {
                let fs = if v.fs_type.is_empty() {
                    "unknown fs"
                } else {
                    v.fs_type.as_str()
                };
                rows.push(("Volume", format!("{} ({fs}, {})", v.device, v.kind.label())));
                rows.push((
                    "Volume usage",
                    format!(
                        "{} used of {} ({})",
                        format::bytes_str(v.used, app.si),
                        format::bytes_str(v.total, app.si),
                        format::percent(v.share())
                    ),
                ));
            }
            None => rows.push(("Volume", "another volume (not descended into)".into())),
        }
    }
    if node.is_dir() && !node.has(crate::scan::flags::OTHER_FS) {
        rows.push((
            "Contents",
            format!(
                "{} items ({} directories, {} files directly inside)",
                format::count(node.items),
                format::count(subdirs as u64),
                format::count(files as u64)
            ),
        ));
    }
    rows.push((
        if node.kind == Kind::Dir {
            "Newest mtime"
        } else {
            "Modified"
        },
        format::mtime(node.mtime, "%Y-%m-%d %H:%M:%S"),
    ));

    let width = 80.min(frame.area().width.saturating_sub(2));
    let inner_w = width.saturating_sub(4) as usize;
    let mut lines: Vec<Line> = Vec::new();
    for (k, v) in &rows {
        let label = format!(" {:<14}", format!("{k}:"));
        let value_w = inner_w.saturating_sub(format::width(&label));
        let mut first = true;
        for chunk in wrap(v, value_w.max(8)) {
            if first {
                lines.push(Line::from(vec![
                    Span::styled(label.clone(), st.help_key),
                    Span::styled(chunk, st.help_desc),
                ]));
                first = false;
            } else {
                lines.push(Line::from(vec![
                    Span::raw(" ".repeat(format::width(&label))),
                    Span::styled(chunk, st.help_desc),
                ]));
            }
        }
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        " any key to close",
        st.keybar_label,
    )));

    let height = (lines.len() as u16 + 2).min(frame.area().height.saturating_sub(2));
    let inner = frame_box(
        frame,
        app,
        &format!("{}Info", icon_or(app, "🔎 ")),
        width,
        height,
        false,
    );
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Simple width-aware wrapping at character boundaries (paths have no spaces to break on).
fn wrap(s: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > width && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
            w = 0;
        }
        cur.push(ch);
        w += cw;
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

fn confirm(frame: &mut Frame, app: &App, mode: DeleteMode, path: &[usize]) {
    let st = &app.theme.styles;
    let Some(tree) = &app.tree else { return };
    let node = crate::app::node_at(tree, path);
    let (title, verb, danger) = match mode {
        DeleteMode::Permanent => (
            format!("{}Delete permanently", icon_or(app, "❌ ")),
            "Permanently delete",
            true,
        ),
        DeleteMode::Trash => (
            format!("{}Move to trash", icon_or(app, "🧹 ")),
            "Move to trash",
            false,
        ),
    };
    let width = 64.min(frame.area().width.saturating_sub(2));
    let name_w = width.saturating_sub(6) as usize;
    let detail = if node.is_dir() {
        format!(
            "{} · {} items",
            format::bytes_str(node.size_of(app.apparent), app.si),
            format::count(node.items)
        )
    } else {
        format::bytes_str(node.size_of(app.apparent), app.si)
    };
    let lines = vec![
        Line::default(),
        Line::from(Span::styled(
            format!("  {verb}"),
            if danger { st.danger } else { st.warning },
        )),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{} ", app.icons.for_node(node)), st.help_desc),
            Span::styled(
                format::truncate_right(&node.name, name_w),
                Style::new().patch(st.help_key),
            ),
        ]),
        Line::from(Span::styled(format!("  {detail}"), st.help_desc)),
        Line::default(),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(" y ", st.keybar_key),
            Span::styled(" confirm   ", st.keybar_label),
            Span::styled(" n ", st.keybar_key),
            Span::styled(" cancel", st.keybar_label),
        ]),
    ];
    let inner = frame_box(frame, app, &title, width, lines.len() as u16 + 2, danger);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn themes(frame: &mut Frame, app: &App, cursor: usize, prompt: Option<&str>) {
    let st = &app.theme.styles;
    let area = frame.area();
    let n = app.themes.len();
    let height = (n as u16 + 5).min(area.height.saturating_sub(2));
    let inner = frame_box(
        frame,
        app,
        &format!("{}Themes", icon_or(app, "🎨 ")),
        60,
        height,
        false,
    );
    let list_h = inner.height.saturating_sub(3) as usize;
    let start = cursor
        .saturating_sub(list_h.saturating_sub(1))
        .min(n.saturating_sub(list_h));

    let mut lines: Vec<Line> = Vec::new();
    for (i, t) in app.themes.iter().enumerate().skip(start).take(list_h) {
        let selected = i == cursor;
        let marker = if selected { "▸ " } else { "  " };
        let mut line = Line::from(vec![
            Span::styled(marker, st.marker),
            Span::styled(
                format::fit(&t.name, 26),
                if selected { st.help_key } else { st.help_desc },
            ),
            Span::styled(
                format::truncate_right(&t.id, inner.width.saturating_sub(30) as usize),
                st.hidden,
            ),
        ]);
        if selected {
            line = line.style(st.selected);
        }
        lines.push(line);
    }
    lines.push(Line::default());
    if let Some(text) = prompt {
        lines.push(Line::from(vec![
            Span::styled(" name for the new theme: ", st.help_desc),
            Span::styled(text.to_string(), st.filter),
            Span::styled("▏", st.filter),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" ⏎ ", st.keybar_key),
            Span::styled(" create  ", st.keybar_label),
            Span::styled(" Esc ", st.keybar_key),
            Span::styled(" cancel", st.keybar_label),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled(" ⏎ ", st.keybar_key),
            Span::styled(" keep  ", st.keybar_label),
            Span::styled(" e ", st.keybar_key),
            Span::styled(" edit  ", st.keybar_label),
            Span::styled(" n ", st.keybar_key),
            Span::styled(" new copy  ", st.keybar_label),
            Span::styled(" S ", st.keybar_key),
            Span::styled(" set default", st.keybar_label),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" Esc ", st.keybar_key),
            Span::styled(" revert", st.keybar_label),
        ]));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn options(frame: &mut Frame, app: &App) {
    let st = &app.theme.styles;
    let check = |on: bool| if on { "[x]" } else { "[ ]" };
    let bar = match app.bar_mode {
        crate::config::BarMode::BarPercent => "bar + percent",
        crate::config::BarMode::Bar => "bar",
        crate::config::BarMode::Percent => "percent",
        crate::config::BarMode::None => "none",
    };
    let icons = match app.config.icons {
        crate::icons::IconMode::Emoji => "emoji",
        crate::icons::IconMode::Ascii => "ascii",
        crate::icons::IconMode::None => "none",
    };
    let view = match app.view {
        crate::config::View::List => "list",
        crate::config::View::Tree => "tree",
    };
    let rows: Vec<(&str, String, String)> = vec![
        (
            "t",
            check(app.dirs_first).into(),
            "directories first".into(),
        ),
        ("y", check(app.group_by_type).into(), "group by type".into()),
        (
            "e",
            check(app.show_hidden).into(),
            "show hidden entries".into(),
        ),
        (
            "a",
            check(app.apparent).into(),
            "apparent size instead of disk usage".into(),
        ),
        (
            "c",
            check(app.show_count).into(),
            "item-count column".into(),
        ),
        ("m", check(app.show_mtime).into(), "mtime column".into()),
        ("b", "   ".into(), format!("bar: {bar}")),
        ("i", "   ".into(), format!("icons: {icons}")),
        ("v", "   ".into(), format!("view: {view}")),
        ("B", check(app.config.borders).into(), "borders".into()),
        (
            "k",
            "   ".into(),
            format!("key guide: {}", app.config.key_guide.label()),
        ),
        (
            "X",
            check(app.config.one_file_system).into(),
            "stay on this volume (other mounts listed, not scanned; toggling rescans)".into(),
        ),
    ];
    let mut lines: Vec<Line> = vec![Line::default()];
    for (k, state, label) in rows {
        lines.push(Line::from(vec![
            Span::styled(format!("  {k}  "), st.help_key),
            Span::styled(format!("{state} "), st.status_accent),
            Span::styled(label, st.help_desc),
        ]));
    }
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled(" Esc ", st.keybar_key),
        Span::styled(" close   ", st.keybar_label),
        Span::styled(
            "changes apply now; put them in config.toml to keep them",
            st.hidden,
        ),
    ]));
    let width = 78.min(frame.area().width.saturating_sub(2));
    let height = (lines.len() as u16 + 2).min(frame.area().height.saturating_sub(2));
    let inner = frame_box(
        frame,
        app,
        &format!("{}Options", icon_or(app, "🔧 ")),
        width,
        height,
        false,
    );
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn editor(frame: &mut Frame, app: &App, ed: &crate::theme_editor::ThemeEditor) {
    use crate::theme_editor::{EditorRow, Field};
    use ratatui::layout::{Constraint, Layout};
    let st = &app.theme.styles;
    let area = frame.area();
    let width = 90.min(area.width.saturating_sub(2));
    let height = area.height.saturating_sub(2);
    let dirty = if ed.dirty { " *" } else { "" };
    let title = format!(
        "{}Edit theme: {} ({}){dirty}",
        icon_or(app, "🎨 "),
        ed.doc.name.as_deref().unwrap_or(&ed.id),
        ed.id
    );
    let inner = frame_box(frame, app, &title, width, height, false);
    if inner.height < 6 {
        return;
    }
    let [head, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(4),
    ])
    .areas(inner);

    frame.render_widget(
        Line::from(vec![
            Span::styled(format!(" {:<14}", "element"), st.hidden),
            Span::styled(format!("{:<13}", "fg"), st.hidden),
            Span::styled(format!("{:<13}", "bg"), st.hidden),
            Span::styled(format!("{:<8}", "attrs"), st.hidden),
            Span::styled("preview", st.hidden),
        ]),
        head,
    );

    let list_h = (body.height as usize).max(1);
    let max_scroll = ed.rows.len().saturating_sub(list_h);
    let scroll = ed.cursor.saturating_sub(list_h - 1).min(max_scroll);
    for (i, row) in ed.rows.iter().enumerate().skip(scroll).take(list_h) {
        let rect = Rect::new(body.x, body.y + (i - scroll) as u16, body.width, 1);
        let selected = i == ed.cursor;
        if selected {
            frame.buffer_mut().set_style(rect, st.selected);
        }
        let p = |s: Style| if selected { s.patch(st.selected) } else { s };
        let mut spans = vec![Span::styled(
            format!("{}{:<13}", if selected { "▸" } else { " " }, row.label()),
            p(if selected { st.help_key } else { st.help_desc }),
        )];
        match row {
            EditorRow::Name => spans.push(Span::styled(
                ed.doc.name.clone().unwrap_or_else(|| ed.id.clone()),
                p(st.help_desc),
            )),
            EditorRow::Dark => spans.push(Span::styled(
                match ed.doc.dark {
                    Some(true) => "yes",
                    Some(false) => "no",
                    None => "unset",
                },
                p(st.help_desc),
            )),
            EditorRow::Style(key) => {
                let def = ed.style_def(key);
                let dash = if ed.defines(key) {
                    "-".to_string()
                } else {
                    "(inherited)".to_string()
                };
                spans.push(Span::styled(
                    format!("{:<13}", def.fg.as_ref().unwrap_or(&dash)),
                    p(st.help_desc),
                ));
                spans.push(Span::styled(
                    format!("{:<13}", def.bg.as_ref().unwrap_or(&dash)),
                    p(st.help_desc),
                ));
                let attrs: String = [
                    (def.bold, 'B'),
                    (def.italic, 'I'),
                    (def.underline, 'U'),
                    (def.dim, 'D'),
                    (def.reversed, 'R'),
                    (def.crossed_out, 'X'),
                ]
                .iter()
                .map(|(on, c)| if *on == Some(true) { *c } else { '·' })
                .collect();
                spans.push(Span::styled(format!("{attrs:<8}"), p(st.help_desc)));
                let sample = app.theme.styles.get(key).unwrap_or_default();
                spans.push(Span::styled(" Sample text 123 ", sample));
            }
            EditorRow::BarFilled | EditorRow::BarEmpty => {
                let v = if *row == EditorRow::BarFilled {
                    &ed.doc.bar.filled
                } else {
                    &ed.doc.bar.empty
                };
                spans.push(Span::styled(
                    format!(
                        "{:<34}",
                        v.clone().unwrap_or_else(|| "- (inherited)".into())
                    ),
                    p(st.help_desc),
                ));
                let b = &app.theme.bar;
                spans.push(Span::styled(b.filled.repeat(6), st.bar_filled));
                spans.push(Span::styled(b.empty.repeat(6), st.bar_empty));
            }
            EditorRow::BarGradient => spans.push(Span::styled(
                match ed.doc.bar.gradient {
                    Some(true) => "on",
                    Some(false) => "off",
                    None => "unset",
                },
                p(st.help_desc),
            )),
            EditorRow::BarLow | EditorRow::BarMid | EditorRow::BarHigh => {
                let (v, colour) = match row {
                    EditorRow::BarLow => (&ed.doc.bar.low, app.theme.bar.low),
                    EditorRow::BarMid => (&ed.doc.bar.mid, app.theme.bar.mid),
                    _ => (&ed.doc.bar.high, app.theme.bar.high),
                };
                spans.push(Span::styled(
                    format!(
                        "{:<34}",
                        v.clone().unwrap_or_else(|| "- (inherited)".into())
                    ),
                    p(st.help_desc),
                ));
                spans.push(Span::styled("██████", Style::new().fg(colour)));
            }
        }
        frame.render_widget(Line::from(spans), rect);
    }

    let mut foot_lines: Vec<Line> = Vec::new();
    if let Some(pr) = &ed.prompt {
        let what = match pr.field {
            Field::Name => "theme name",
            Field::Fg => "foreground colour",
            Field::Bg => "background colour",
            Field::BarFilled => "filled glyph (one cell)",
            Field::BarEmpty => "empty glyph (one cell)",
            Field::BarLow => "gradient low colour",
            Field::BarMid => "gradient mid colour",
            Field::BarHigh => "gradient high colour",
        };
        foot_lines.push(Line::from(vec![
            Span::styled(format!(" {what} for {}: ", ed.row().label()), st.help_desc),
            Span::styled(pr.text.clone(), st.filter),
            Span::styled("▏", st.filter),
        ]));
        let current = if pr.current.is_empty() {
            "(inherited)"
        } else {
            pr.current.as_str()
        };
        foot_lines.push(Line::from(Span::styled(
            format!(
                " current: {current} · #rrggbb, rgb(r,g,b), 0-255, colour name or palette key · none = inherit · empty = keep"
            ),
            st.hidden,
        )));
    } else {
        foot_lines.push(Line::from(vec![
            Span::styled(" f ", st.keybar_key),
            Span::styled(" fg picker ", st.keybar_label),
            Span::styled(" g ", st.keybar_key),
            Span::styled(" bg picker ", st.keybar_label),
            Span::styled(" F G ", st.keybar_key),
            Span::styled(" type value ", st.keybar_label),
            Span::styled(" b i u d r x ", st.keybar_key),
            Span::styled(
                " bold italic underline dim reversed strike ",
                st.keybar_label,
            ),
            Span::styled(" Del ", st.keybar_key),
            Span::styled(" inherit", st.keybar_label),
        ]));
        foot_lines.push(Line::from(vec![
            Span::styled(" ⏎ ", st.keybar_key),
            Span::styled(" edit/toggle ", st.keybar_label),
            Span::styled(" s ", st.keybar_key),
            Span::styled(" save ", st.keybar_label),
            Span::styled(" Esc ", st.keybar_key),
            Span::styled(" close ", st.keybar_label),
        ]));
    }
    let palette: Vec<&str> = ed.doc.palette.keys().map(|k| k.as_str()).collect();
    let palette_line = if palette.is_empty() {
        " palette: (none defined)".to_string()
    } else {
        format!(" palette: {}", palette.join(" "))
    };
    foot_lines.push(Line::from(Span::styled(
        format::truncate_right(&palette_line, foot.width as usize),
        st.hidden,
    )));
    match &ed.message {
        Some((m, warn)) => foot_lines.push(Line::from(Span::styled(
            format!(
                " {}",
                format::truncate_right(m, foot.width.saturating_sub(2) as usize)
            ),
            if *warn { st.warning } else { st.success },
        ))),
        None => foot_lines.push(Line::default()),
    }
    frame.render_widget(Paragraph::new(Text::from(foot_lines)), foot);

    if let Some(picker) = &ed.picker {
        color_picker(frame, app, ed, picker);
    }
}

/// The RGB / HSV slider modal drawn over the theme editor.
fn color_picker(
    frame: &mut Frame,
    app: &App,
    ed: &crate::theme_editor::ThemeEditor,
    pk: &crate::theme_editor::ColorPicker,
) {
    use crate::theme_editor::{CHANNELS, EditorRow, Field, nearest_ansi, nearest_xterm256};
    use ratatui::style::Color;
    let st = &app.theme.styles;
    let width = 78.min(frame.area().width.saturating_sub(2));
    let height = 19.min(frame.area().height.saturating_sub(2));
    let inner = frame_box(
        frame,
        app,
        &format!("{}Colour: {}", icon_or(app, "🎨 "), pk.label),
        width,
        height,
        false,
    );
    let slider_w = (inner.width as usize).saturating_sub(18).clamp(8, 40);
    let mut lines: Vec<Line> = vec![Line::default()];

    for (ch, name) in CHANNELS.iter().enumerate() {
        if ch == 3 {
            lines.push(Line::default());
        }
        let active = ch == pk.channel;
        let value = pk.channel_value(ch);
        let (max, unit) = match ch {
            0..=2 => (255.0, ""),
            3 => (359.0, "°"),
            _ => (100.0, "%"),
        };
        let pos = ((value as f64 / max) * (slider_w as f64 - 1.0)).round() as usize;
        let mut spans = vec![
            Span::styled(if active { " ▸ " } else { "   " }, st.marker),
            Span::styled(
                format!("{name} "),
                if active { st.help_key } else { st.help_desc },
            ),
        ];
        for i in 0..slider_w {
            let colour = pk.color_at(ch, i as f64 / (slider_w as f64 - 1.0));
            let glyph = if i == pos {
                "┃"
            } else if i < pos {
                "█"
            } else {
                "░"
            };
            spans.push(Span::styled(glyph, Style::new().fg(colour)));
        }
        spans.push(Span::styled(
            format!(" {value:>3}{unit}"),
            if active { st.help_key } else { st.help_desc },
        ));
        lines.push(Line::from(spans));
    }

    lines.push(Line::default());
    let rgb = pk.rgb;
    let swatch = Color::Rgb(rgb[0], rgb[1], rgb[2]);
    let mut value_line = vec![
        Span::styled("   value ", st.help_desc),
        Span::styled(pk.value_string(), st.help_key),
    ];
    if pk.literal.is_some() {
        value_line.push(Span::styled(format!("  ({})", pk.hex()), st.hidden));
    }
    value_line.push(Span::styled(
        format!(
            "   rgb({}, {}, {})   ansi {}   256 #{}",
            rgb[0],
            rgb[1],
            rgb[2],
            nearest_ansi(rgb),
            nearest_xterm256(rgb)
        ),
        st.hidden,
    ));
    lines.push(Line::from(value_line));

    // Preview against the element's other colour.
    let (fg, bg) = match (pk.field, ed.row()) {
        (Field::Fg, EditorRow::Style(k)) => {
            (Some(swatch), app.theme.styles.get(k).and_then(|s| s.bg))
        }
        (Field::Bg, EditorRow::Style(k)) => {
            (app.theme.styles.get(k).and_then(|s| s.fg), Some(swatch))
        }
        _ => (Some(swatch), None),
    };
    let mut preview = Style::new();
    if let Some(f) = fg {
        preview = preview.fg(f);
    }
    if let Some(b) = bg {
        preview = preview.bg(b);
    }
    lines.push(Line::from(vec![
        Span::styled("   preview ", st.help_desc),
        Span::styled("████", Style::new().fg(swatch)),
        Span::styled(" Sample text 123 ", preview),
        Span::styled("████", Style::new().fg(swatch)),
        Span::styled(
            match pk.field {
                Field::BarLow | Field::BarMid | Field::BarHigh => "  ██████░░░░ bar",
                _ => "",
            },
            Style::new().fg(swatch),
        ),
    ]));

    lines.push(Line::default());
    match &pk.entry {
        Some(text) => {
            lines.push(Line::from(vec![
                Span::styled("   type a colour: ", st.help_desc),
                Span::styled(text.clone(), st.filter),
                Span::styled("▏", st.filter),
            ]));
            lines.push(Line::from(Span::styled(
                "   #rrggbb · rgb(r,g,b) · 0-255 · ansi name · palette key   ⏎ apply  Esc back",
                st.hidden,
            )));
        }
        None => {
            lines.push(Line::from(vec![
                Span::styled(" ↑↓ ", st.keybar_key),
                Span::styled(" slider ", st.keybar_label),
                Span::styled(" ←→ ", st.keybar_key),
                Span::styled(" ±1 ", st.keybar_label),
                Span::styled(" H L ", st.keybar_key),
                Span::styled(" ±10 ", st.keybar_label),
                Span::styled(" PgUp PgDn ", st.keybar_key),
                Span::styled(" ±16 ", st.keybar_label),
                Span::styled(" Home End ", st.keybar_key),
                Span::styled(" ends ", st.keybar_label),
                Span::styled(" Tab ", st.keybar_key),
                Span::styled(" RGB/HSV", st.keybar_label),
            ]));
            lines.push(Line::from(vec![
                Span::styled(" # ", st.keybar_key),
                Span::styled(" hex ", st.keybar_label),
                Span::styled(" n ", st.keybar_key),
                Span::styled(" name/palette ", st.keybar_label),
                Span::styled(" ⏎ ", st.keybar_key),
                Span::styled(" apply ", st.keybar_label),
                Span::styled(" Esc ", st.keybar_key),
                Span::styled(" cancel", st.keybar_label),
            ]));
        }
    }
    if let Some((m, warn)) = &ed.message {
        lines.push(Line::from(Span::styled(
            format!(
                "   {}",
                format::truncate_right(m, inner.width.saturating_sub(4) as usize)
            ),
            if *warn { st.warning } else { st.success },
        )));
    }
    let palette: Vec<&str> = ed.doc.palette.keys().map(|k| k.as_str()).collect();
    if !palette.is_empty() {
        lines.push(Line::from(Span::styled(
            format::truncate_right(
                &format!("   palette: {}", palette.join(" ")),
                inner.width as usize,
            ),
            st.hidden,
        )));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn message(frame: &mut Frame, app: &App, title: &str, body: &str, danger: bool) {
    let st = &app.theme.styles;
    let width = 70.min(frame.area().width.saturating_sub(2));
    let body_lines = body.lines().count() as u16 + body.len() as u16 / width.max(1) + 4;
    let height = body_lines.min(frame.area().height.saturating_sub(2));
    let icon = if danger {
        icon_or(app, "❗ ")
    } else {
        icon_or(app, "✅ ")
    };
    let inner = frame_box(frame, app, &format!("{icon}{title}"), width, height, danger);
    let mut text = Text::from(body.to_string()).style(st.help_desc);
    text.push_line(Line::default());
    text.push_line(Line::from(Span::styled(
        "any key to close",
        st.keybar_label,
    )));
    frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), inner);
}
