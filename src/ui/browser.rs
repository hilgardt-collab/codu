//! The main browser screen: header, list or tree rows, status bar and key bar.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Widget};

use crate::app::{App, Row, TreeRow, node_at};
use crate::config::View;
use crate::format;
use crate::scan::{Kind, Node, flags};
use crate::theme;
use crate::volumes::Volume;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let guide = keybar_lines(app, area.width);
    // Always leave room for the header, at least one list row and the status.
    let max_guide = area.height.saturating_sub(4).max(1) as usize;
    let guide_h = guide.len().min(max_guide) as u16;
    let [header, body, status, keybar] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(guide_h),
    ])
    .areas(area);

    draw_header(frame, header, app);
    draw_rows(frame, body, app);
    draw_status(frame, status, app);
    draw_keybar(frame, keybar, app, guide);
}

// ----------------------------------------------------------------------
// Header

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let st = &app.theme.styles;
    frame.buffer_mut().set_style(area, st.header);

    let title = if app.icons.emoji() {
        format!(" {} cdu ", app.icons.app)
    } else {
        " cdu ".to_string()
    };
    let mood = match (app.icons.emoji(), app.theme.dark) {
        (false, _) => "",
        (true, Some(true)) => "🌙 ",
        (true, Some(false)) => "🌞 ",
        (true, None) => "🎨 ",
    };
    let theme_label = format!(" {mood}{} ", app.theme.name);

    let right_w = format::width(&theme_label) as u16;
    let title_w = format::width(&title) as u16;
    let avail = area.width.saturating_sub(right_w + title_w + 2) as usize;
    let path = app.header_path().display().to_string();
    let path = format!(" {} ", format::truncate_left(&path, avail));

    frame.render_widget(
        Line::from(vec![
            Span::styled(title, st.header_title),
            Span::styled(path, st.header_path),
        ]),
        area,
    );
    if area.width > right_w + title_w + 4 {
        frame.render_widget(
            Line::from(Span::styled(theme_label, st.header_theme)).right_aligned(),
            area,
        );
    }
}

// ----------------------------------------------------------------------
// Columns

/// Column widths (each includes its trailing gap, except `name`).
#[derive(Clone, Copy, Debug, Default)]
struct Cols {
    marker: u16,
    flag: u16,
    icon: u16,
    name: u16,
    size: u16,
    bar: u16,
    percent: u16,
    count: u16,
    mtime: u16,
}

const MIN_NAME: i32 = 16;
const MIN_BAR: i32 = 6;

fn layout_columns(width: u16, app: &App, mtime_w: u16) -> Cols {
    let mut c = Cols {
        marker: 2,
        flag: 2,
        icon: app.icons.column_width(),
        size: 10,
        bar: if app.bar_mode.show_bar() {
            app.bar_width + 1
        } else {
            0
        },
        percent: if app.bar_mode.show_percent() { 7 } else { 0 },
        count: if app.show_count { 10 } else { 0 },
        mtime: if app.show_mtime { mtime_w + 1 } else { 0 },
        ..Cols::default()
    };
    let name_for = |c: &Cols| {
        width as i32
            - (c.marker + c.flag + c.icon + c.size + c.bar + c.percent + c.count + c.mtime) as i32
    };
    if name_for(&c) < MIN_NAME {
        c.mtime = 0;
    }
    if name_for(&c) < MIN_NAME {
        c.count = 0;
    }
    if name_for(&c) < MIN_NAME && c.bar > 0 {
        let spare = name_for(&Cols { bar: 0, ..c }) - MIN_NAME;
        c.bar = if spare > MIN_BAR {
            spare.min(c.bar as i32) as u16
        } else {
            0
        };
    }
    if name_for(&c) < MIN_NAME {
        c.percent = 0;
    }
    if name_for(&c) < MIN_NAME {
        c.flag = 0;
    }
    c.name = name_for(&c).max(4) as u16;
    c
}

// ----------------------------------------------------------------------
// Rows

fn draw_rows(frame: &mut Frame, area: Rect, app: &mut App) {
    let st = app.theme.styles.clone();
    let inner = if app.config.borders {
        let position = if app.row_count() == 0 {
            String::new()
        } else {
            format!(" {}/{} ", app.cursor + 1, app.row_count())
        };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(st.border)
            .title_bottom(Line::from(position).style(st.border_title).right_aligned());
        let inner = block.inner(area);
        frame.render_widget(block, area);
        inner
    } else {
        area
    };
    app.list_area = inner;
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    app.ensure_visible(inner.height as usize);

    let mtime_sample = format::mtime(0, &app.config.date_format);
    let cols = layout_columns(inner.width, app, format::width(&mtime_sample) as u16);

    match app.view {
        View::List => draw_list_rows(frame, inner, app, &cols),
        View::Tree => draw_tree_rows(frame, inner, app, &cols),
    }
}

fn draw_list_rows(frame: &mut Frame, inner: Rect, app: &App, cols: &Cols) {
    let st = &app.theme.styles;
    let max_size = app.max_listed_size();
    let Some(dir) = app.current() else { return };
    let dir_size = dir.size_of(app.apparent);

    if app.rows.len() <= 1 {
        let msg = if !app.filter.is_empty() {
            "no entries match the filter"
        } else if dir.has(flags::ERR) {
            "directory could not be read"
        } else {
            "empty directory"
        };
        frame.render_widget(
            Line::from(Span::styled(msg, st.hidden)).centered(),
            Rect::new(inner.x, inner.y + inner.height / 2, inner.width, 1),
        );
        if inner.height < 2 {
            return;
        }
    }

    let buf = frame.buffer_mut();
    let end = (app.scroll + inner.height as usize).min(app.rows.len());
    for (i, row) in app.rows[app.scroll..end].iter().enumerate() {
        let rect = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        let selected = app.scroll + i == app.cursor;
        let sel = if selected { st.selected } else { Style::new() };
        if selected {
            buf.set_style(rect, st.selected);
        }
        let p = |s: Style| s.patch(sel);

        let mut spans: Vec<Span> = Vec::with_capacity(14);
        spans.push(Span::styled(
            if selected { "▸ " } else { "  " },
            p(st.marker),
        ));
        match row {
            Row::Parent => push_parent(&mut spans, app, cols, cols.name as usize, p),
            Row::Group(g) => {
                let g = &app.groups[*g];
                let icon = if g.icon.is_empty() {
                    String::new()
                } else {
                    format!("{} ", g.icon)
                };
                let caption = format!(
                    "─ {icon}{} · {} · {} ",
                    g.label,
                    format::count(g.count as u64),
                    format::bytes_str(g.size, app.si)
                );
                let avail = (inner.width as usize).saturating_sub(2);
                let caption = format::truncate_right(&caption, avail);
                let fill = avail.saturating_sub(format::width(&caption));
                spans.push(Span::styled(caption, p(st.group)));
                spans.push(Span::styled("─".repeat(fill), p(st.group)));
            }
            Row::Entry(idx) => {
                let node = &dir.children[*idx];
                let name_style =
                    push_entry_head(&mut spans, app, node, cols, "", cols.name as usize, p);
                let size = node.size_of(app.apparent);
                let fill = if max_size > 0 {
                    size as f64 / max_size as f64
                } else {
                    0.0
                };
                let share = if dir_size > 0 {
                    size as f64 / dir_size as f64
                } else {
                    0.0
                };
                let mount = if node.has(flags::OTHER_FS) {
                    app.volume_for(&app.current_path().join(&*node.name))
                } else {
                    None
                };
                push_metrics(&mut spans, app, node, mount, fill, share, cols, p);
                let _ = name_style;
            }
        }
        Line::from(spans).render(rect, buf);
    }
}

fn draw_tree_rows(frame: &mut Frame, inner: Rect, app: &App, cols: &Cols) {
    let st = &app.theme.styles;
    let Some(tree) = &app.tree else { return };
    let root_size = tree.size_of(app.apparent);

    let buf = frame.buffer_mut();
    let end = (app.scroll + inner.height as usize).min(app.tree_rows.len());
    for (i, row) in app.tree_rows[app.scroll..end].iter().enumerate() {
        let rect = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        let selected = app.scroll + i == app.cursor;
        let sel = if selected { st.selected } else { Style::new() };
        if selected {
            buf.set_style(rect, st.selected);
        }
        let p = |s: Style| s.patch(sel);

        let mut spans: Vec<Span> = Vec::with_capacity(16);
        spans.push(Span::styled(
            if selected { "▸ " } else { "  " },
            p(st.marker),
        ));
        if row.parent {
            push_parent(&mut spans, app, cols, cols.name as usize, p);
        } else {
            let node = node_at(tree, &row.path);
            let prefix = tree_prefix(row, node, cols.name as usize);
            let name_w = (cols.name as usize).saturating_sub(format::width(&prefix));
            push_entry_head(&mut spans, app, node, cols, &prefix, name_w, p);
            if row.path.is_empty() {
                // The root row shows the full scanned path instead of its name.
                if let Some(last) = spans.last_mut() {
                    let label = app.root_path.display().to_string();
                    last.content = format::fit(&label, name_w).into();
                }
            }
            let size = node.size_of(app.apparent);
            let share = if root_size > 0 {
                size as f64 / root_size as f64
            } else {
                0.0
            };
            let mount = if node.has(flags::OTHER_FS) {
                app.volume_for(&app.fs_path(&row.path))
            } else {
                None
            };
            push_metrics(&mut spans, app, node, mount, share, share, cols, p);
        }
        Line::from(spans).render(rect, buf);
    }
}

/// Guide lines and the +/- toggle in front of a tree row's icon.
fn tree_prefix(row: &TreeRow, node: &Node, name_area: usize) -> String {
    let toggle = if node.is_dir() {
        if node.expanded { "- " } else { "+ " }
    } else {
        "  "
    };
    if row.path.is_empty() {
        return toggle.to_string();
    }
    let mut guides = String::new();
    for &last in &row.guides {
        guides.push_str(if last { "   " } else { "│  " });
    }
    guides.push_str(if row.is_last { "└─" } else { "├─" });
    let full = format!("{guides}{toggle}");
    // Deep trees: keep the name readable by compressing the guides.
    if format::width(&full) + 12 > name_area {
        format!("…{} {toggle}", row.depth())
    } else {
        full
    }
}

/// The `..` row.
fn push_parent<'a>(
    spans: &mut Vec<Span<'a>>,
    app: &'a App,
    cols: &Cols,
    name_w: usize,
    p: impl Fn(Style) -> Style,
) {
    let st = &app.theme.styles;
    if cols.flag > 0 {
        spans.push(Span::raw("  "));
    }
    if cols.icon > 0 {
        spans.push(Span::styled(format!("{} ", app.icons.parent), p(st.dir)));
    }
    spans.push(Span::styled(format::fit("..", name_w), p(st.dir)));
}

/// Flag, icon and name for an entry. Returns the name style used.
fn push_entry_head<'a>(
    spans: &mut Vec<Span<'a>>,
    app: &'a App,
    node: &'a Node,
    cols: &Cols,
    prefix: &str,
    name_w: usize,
    p: impl Fn(Style) -> Style,
) -> Style {
    let st = &app.theme.styles;
    let hidden = node.name.starts_with('.');
    let name_style = if node.has(flags::EXCLUDED) {
        st.excluded
    } else if node.has(flags::ERR) {
        st.error
    } else if hidden {
        st.hidden
    } else {
        match node.kind {
            Kind::Dir => st.dir,
            Kind::File => st.file,
            Kind::Symlink => st.symlink,
            Kind::Other => st.special,
        }
    };

    if cols.flag > 0 {
        let flag = node
            .flag_char()
            .map(|c| format!("{c} "))
            .unwrap_or_else(|| "  ".into());
        let flag_style = if node.has(flags::ERR) {
            st.error
        } else {
            st.flag
        };
        spans.push(Span::styled(flag, p(flag_style)));
    }
    if !prefix.is_empty() {
        // Guides in one style, the trailing "+ " / "- " toggle in another.
        let split = prefix.len() - 2;
        let (guides, toggle) = prefix.split_at(split);
        spans.push(Span::styled(guides.to_string(), p(st.tree_guide)));
        spans.push(Span::styled(toggle.to_string(), p(st.tree_toggle)));
    }
    if cols.icon > 0 {
        let icon = if prefix.is_empty() {
            app.icons.for_node(node)
        } else {
            app.icons.for_tree_node(node)
        };
        spans.push(Span::styled(format!("{icon} "), p(name_style)));
    }
    spans.push(Span::styled(format::fit(&node.name, name_w), p(name_style)));
    name_style
}

/// Size, bar, percent, count and mtime columns.
#[allow(clippy::too_many_arguments)]
fn push_metrics<'a>(
    spans: &mut Vec<Span<'a>>,
    app: &'a App,
    node: &Node,
    mount: Option<&Volume>,
    fill: f64,
    share: f64,
    cols: &Cols,
    p: impl Fn(Style) -> Style,
) {
    let st = &app.theme.styles;
    // A mount point of a volume we know nothing about (pseudo filesystems
    // such as /proc, or a platform without volume listing).
    if mount.is_none() && node.has(flags::OTHER_FS) {
        spans.push(Span::styled(format!(" {:>5} ", "-"), p(st.mount)));
        spans.push(Span::styled(format!("{:<3}", ""), p(st.mount)));
        if cols.bar > 0 {
            let width = (cols.bar - 1) as usize;
            spans.push(Span::raw(" "));
            spans.push(Span::styled(format::fit("mount point", width), p(st.mount)));
        }
        if cols.percent > 0 {
            spans.push(Span::styled(format!(" {:>6}", "-"), p(st.mount)));
        }
        if cols.count > 0 {
            spans.push(Span::styled(format!(" {:>9}", ""), p(st.count)));
        }
        if cols.mtime > 0 {
            let text = format::mtime(node.mtime, &app.config.date_format);
            spans.push(Span::styled(
                format!(" {}", format::fit(&text, (cols.mtime - 1) as usize)),
                p(st.mtime),
            ));
        }
        return;
    }
    // A mount point of another volume: show that volume's own usage, in the
    // mount style, and leave the bar/percent empty since it is not part of
    // this scan's totals.
    if let Some(vol) = mount {
        let (num, unit) = format::bytes(vol.used, app.si);
        spans.push(Span::styled(format!(" {num:>5} "), p(st.mount)));
        spans.push(Span::styled(format!("{unit:<3}"), p(st.mount)));
        if cols.bar > 0 {
            let width = (cols.bar - 1) as usize;
            let label = format::truncate_right(&format!("volume {}", vol.short_device()), width);
            spans.push(Span::raw(" "));
            spans.push(Span::styled(format::fit(&label, width), p(st.mount)));
        }
        if cols.percent > 0 {
            spans.push(Span::styled(
                format!(" {:>6}", format::percent(vol.share())),
                p(st.mount),
            ));
        }
        if cols.count > 0 {
            spans.push(Span::styled(format!(" {:>9}", ""), p(st.count)));
        }
        if cols.mtime > 0 {
            let text = format::mtime(node.mtime, &app.config.date_format);
            spans.push(Span::styled(
                format!(" {}", format::fit(&text, (cols.mtime - 1) as usize)),
                p(st.mtime),
            ));
        }
        return;
    }
    let size = node.size_of(app.apparent);
    let (num, unit) = format::bytes(size, app.si);
    spans.push(Span::styled(format!(" {num:>5} "), p(st.size)));
    spans.push(Span::styled(format!("{unit:<3}"), p(st.size_unit)));

    if cols.bar > 0 {
        let width = (cols.bar - 1) as usize;
        let filled = ((fill.clamp(0.0, 1.0)) * width as f64).round() as usize;
        let bar = &app.theme.bar;
        let filled_style = if bar.gradient {
            Style::new().fg(theme::gradient(bar, share))
        } else {
            st.bar_filled
        };
        spans.push(Span::raw(" "));
        spans.push(Span::styled(bar.filled.repeat(filled), p(filled_style)));
        spans.push(Span::styled(
            bar.empty.repeat(width - filled),
            p(st.bar_empty),
        ));
    }
    if cols.percent > 0 {
        spans.push(Span::styled(
            format!(" {:>6}", format::percent(share)),
            p(st.percent),
        ));
    }
    if cols.count > 0 {
        let text = if node.is_dir() {
            format::count(node.items)
        } else {
            String::new()
        };
        spans.push(Span::styled(format!(" {text:>9}"), p(st.count)));
    }
    if cols.mtime > 0 {
        let text = format::mtime(node.mtime, &app.config.date_format);
        spans.push(Span::styled(
            format!(" {}", format::fit(&text, (cols.mtime - 1) as usize)),
            p(st.mtime),
        ));
    }
}

// ----------------------------------------------------------------------
// Status bar

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let st = &app.theme.styles;
    frame.buffer_mut().set_style(area, st.status);
    let node = match app.view {
        View::List => app.current(),
        View::Tree => app.tree.as_ref(),
    };
    let Some(dir) = node else { return };
    let icons = app.icons.emoji();

    let mut spans: Vec<Span> = vec![Span::raw(" ")];
    spans.push(Span::styled(if icons { "💾 " } else { "" }, st.status));
    spans.push(Span::styled(
        format::bytes_str(dir.size_of(app.apparent), app.si),
        st.status_accent,
    ));
    spans.push(Span::styled(" in ", st.status));
    spans.push(Span::styled(format::count(dir.items), st.status_accent));
    spans.push(Span::styled(" items", st.status));

    spans.push(Span::styled("  ·  ", st.status));
    spans.push(Span::styled(
        match app.view {
            View::List => "list",
            View::Tree => "tree",
        },
        st.status,
    ));

    spans.push(Span::styled("  ·  ", st.status));
    let arrow = if app.sort_reverse { "↑" } else { "↓" };
    spans.push(Span::styled(format!("{arrow} "), st.status_accent));
    spans.push(Span::styled(app.sort.label(), st.status));
    if app.dirs_first {
        spans.push(Span::styled(" (dirs first)", st.status));
    }
    if app.group_by_type {
        spans.push(Span::styled(" (by type)", st.status));
    }

    spans.push(Span::styled("  ·  ", st.status));
    spans.push(Span::styled(
        if app.apparent {
            "apparent size"
        } else {
            "disk usage"
        },
        st.status,
    ));
    spans.push(Span::styled("  ·  ", st.status));
    spans.push(Span::styled(
        if app.config.one_file_system {
            "this volume"
        } else {
            "all volumes"
        },
        st.status,
    ));

    if !app.filter.is_empty() || app.filter_editing {
        spans.push(Span::styled("  ·  ", st.status));
        spans.push(Span::styled(
            if icons { "🔍 " } else { "filter: " },
            st.filter,
        ));
        spans.push(Span::styled(app.filter.clone(), st.filter));
        if app.filter_editing {
            spans.push(Span::styled("▏", st.filter));
        }
    }

    let left_w: usize = spans.iter().map(|s| format::width(&s.content)).sum();
    frame.render_widget(Line::from(spans), area);

    if let Some(msg) = &app.status {
        let style = if msg.warn { st.warning } else { st.success };
        let room = (area.width as usize).saturating_sub(left_w + 3);
        if room >= 8 {
            let text = format!(" {} ", format::truncate_right(&msg.text, room));
            frame.render_widget(Line::from(Span::styled(text, style)).right_aligned(), area);
        }
    }
}

// ----------------------------------------------------------------------
// Key guide

/// Every shortcut that works in the current context, in display order.
pub fn key_pairs(app: &App) -> Vec<(&'static str, &'static str)> {
    if app.filter_editing {
        return vec![
            ("type", "filter text"),
            ("⏎", "apply"),
            ("Esc", "clear"),
            ("↑↓", "move"),
        ];
    }
    let mut v: Vec<(&str, &str)> = vec![
        ("↑↓ jk", "move"),
        ("PgUp PgDn", "page"),
        ("Home End", "first/last"),
    ];
    match app.view {
        View::List => v.extend([("⏎ →", "open"), ("⌫ ←", "up"), ("Tab", "tree view")]),
        View::Tree => v.extend([
            ("⏎ Space", "toggle"),
            ("+ →", "expand"),
            ("- ←", "collapse"),
            ("*", "expand all"),
            ("⌫", "parent"),
            ("Tab", "list view"),
        ]),
    }
    v.extend([
        ("/", "filter"),
        ("s", "sort size"),
        ("n", "sort name"),
        ("C", "sort items"),
        ("M", "sort mtime"),
        ("t", "dirs first"),
        ("y", "by type"),
        ("a", "apparent"),
        ("b", "bar"),
        ("c", "count col"),
        ("m", "mtime col"),
        ("e", "hidden"),
        ("i", "info"),
        ("d", "delete"),
        ("D", "trash"),
        ("r", "rescan"),
        ("o", "options"),
        ("V", "volumes"),
        ("T", "themes"),
        ("?", "help"),
        ("Esc", "quit"),
    ]);
    v
}

/// Lay the key pairs out as chips, wrapping at pair boundaries so nothing is
/// cut mid-word. In compact mode this is a single line, truncated.
fn keybar_lines(app: &App, width: u16) -> Vec<Line<'static>> {
    guide_lines(app, key_pairs(app), width)
}

/// Lay out an explicit list of key pairs (shared with the volumes screen).
pub fn guide_lines(
    app: &App,
    pairs: Vec<(&'static str, &'static str)>,
    width: u16,
) -> Vec<Line<'static>> {
    use crate::config::KeyGuide;
    let st = &app.theme.styles;
    let mode = app.config.key_guide;
    if mode == KeyGuide::Off || width == 0 {
        return Vec::new();
    }
    let mut lines: Vec<Line> = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut used: usize = 0;
    for (key, label) in pairs {
        let k = format!(" {key} ");
        let l = format!(" {label}  ");
        let w = format::width(&k) + format::width(&l);
        if used + w > width as usize && used > 0 {
            if mode == KeyGuide::Compact {
                break;
            }
            lines.push(Line::from(std::mem::take(&mut spans)));
            used = 0;
        }
        used += w;
        spans.push(Span::styled(k, st.keybar_key));
        spans.push(Span::styled(l, st.keybar_label));
    }
    if !spans.is_empty() {
        lines.push(Line::from(spans));
    }
    lines
}

pub fn draw_keybar(frame: &mut Frame, area: Rect, app: &App, lines: Vec<Line<'static>>) {
    if area.height == 0 {
        return;
    }
    frame.buffer_mut().set_style(area, app.theme.styles.status);
    for (i, line) in lines.into_iter().take(area.height as usize).enumerate() {
        frame.render_widget(line, Rect::new(area.x, area.y + i as u16, area.width, 1));
    }
}
