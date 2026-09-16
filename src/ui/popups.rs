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
        Popup::Themes { cursor, .. } => themes(frame, app, *cursor),
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
    ("a", "toggle disk usage / apparent size"),
    ("b", "cycle bar: bar+percent, bar, percent, none"),
    ("c  m", "toggle item-count / mtime column"),
    ("e", "toggle hidden entries"),
    ("T", "theme picker (live preview)"),
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
        &format!("{}Help", icon_or(app, "❓ ")),
        66,
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
    if node.is_dir() {
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

fn themes(frame: &mut Frame, app: &App, cursor: usize) {
    let st = &app.theme.styles;
    let area = frame.area();
    let n = app.themes.len();
    let height = (n as u16 + 4).min(area.height.saturating_sub(2));
    let inner = frame_box(
        frame,
        app,
        &format!("{}Themes", icon_or(app, "🎨 ")),
        48,
        height,
        false,
    );
    let list_h = inner.height.saturating_sub(2) as usize;
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
    lines.push(Line::from(vec![
        Span::styled(" ⏎ ", st.keybar_key),
        Span::styled(" keep  ", st.keybar_label),
        Span::styled(" Esc ", st.keybar_key),
        Span::styled(" revert", st.keybar_label),
    ]));
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
