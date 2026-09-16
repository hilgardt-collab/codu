//! The main browser screen: header, file list, status bar and key bar.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};

use crate::app::{App, Row};
use crate::config::SortKey;
use crate::format;
use crate::scan::{Kind, Node, flags};
use crate::theme;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let [header, body, status, keybar] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_header(frame, header, app);
    draw_list(frame, body, app);
    draw_status(frame, status, app);
    draw_keybar(frame, keybar, app);
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
    let path = app.current_path().display().to_string();
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
// List

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

fn draw_list(frame: &mut Frame, area: Rect, app: &mut App) {
    let st = app.theme.styles.clone();
    let inner = if app.config.borders {
        let position = if app.rows.is_empty() {
            String::new()
        } else {
            format!(" {}/{} ", app.cursor + 1, app.rows.len())
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
    let max_size = app.max_listed_size();
    let Some(dir) = app.current() else { return };
    let dir_size = dir.size_of(app.apparent);

    if app.rows.is_empty() {
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
        return;
    }

    let buf = frame.buffer_mut();
    let end = (app.scroll + inner.height as usize).min(app.rows.len());
    for (i, row) in app.rows[app.scroll..end].iter().enumerate() {
        let rect = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        let selected = app.scroll + i == app.cursor;
        render_row(
            buf, rect, app, dir, *row, selected, &cols, max_size, dir_size,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_row(
    buf: &mut Buffer,
    rect: Rect,
    app: &App,
    dir: &Node,
    row: Row,
    selected: bool,
    cols: &Cols,
    max_size: u64,
    dir_size: u64,
) {
    let st = &app.theme.styles;
    let sel = if selected { st.selected } else { Style::new() };
    if selected {
        buf.set_style(rect, st.selected);
    }
    let p = |s: Style| s.patch(sel);

    let mut spans: Vec<Span> = Vec::with_capacity(12);
    spans.push(Span::styled(
        if selected { "▸ " } else { "  " },
        p(st.marker),
    ));

    match row {
        Row::Parent => {
            if cols.flag > 0 {
                spans.push(Span::raw("  "));
            }
            if cols.icon > 0 {
                spans.push(Span::styled(format!("{} ", app.icons.parent), p(st.dir)));
            }
            spans.push(Span::styled(
                format::fit("..", cols.name as usize),
                p(st.dir),
            ));
        }
        Row::Entry(i) => {
            let node = &dir.children[i];
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
            if cols.icon > 0 {
                spans.push(Span::styled(
                    format!("{} ", app.icons.for_node(node)),
                    p(name_style),
                ));
            }
            spans.push(Span::styled(
                format::fit(&node.name, cols.name as usize),
                p(name_style),
            ));

            let size = node.size_of(app.apparent);
            let (num, unit) = format::bytes(size, app.si);
            spans.push(Span::styled(format!(" {num:>5} "), p(st.size)));
            spans.push(Span::styled(format!("{unit:<3}"), p(st.size_unit)));

            let share = if dir_size > 0 {
                size as f64 / dir_size as f64
            } else {
                0.0
            };
            if cols.bar > 0 {
                let width = (cols.bar - 1) as usize;
                let filled = if max_size > 0 {
                    ((size as f64 / max_size as f64) * width as f64).round() as usize
                } else {
                    0
                }
                .min(width);
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
    }

    Line::from(spans).render_to(buf, rect);
}

trait RenderTo {
    fn render_to(self, buf: &mut Buffer, rect: Rect);
}

impl RenderTo for Line<'_> {
    fn render_to(self, buf: &mut Buffer, rect: Rect) {
        ratatui::widgets::Widget::render(self, rect, buf);
    }
}

// ----------------------------------------------------------------------
// Status bar

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let st = &app.theme.styles;
    frame.buffer_mut().set_style(area, st.status);
    let Some(dir) = app.current() else { return };
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
    let arrow = match (app.sort, app.sort_reverse) {
        (SortKey::Name, false) => "↓",
        (SortKey::Name, true) => "↑",
        (_, false) => "↓",
        (_, true) => "↑",
    };
    spans.push(Span::styled(format!("{arrow} "), st.status_accent));
    spans.push(Span::styled(app.sort.label(), st.status));
    if app.dirs_first {
        spans.push(Span::styled(" (dirs first)", st.status));
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
// Key bar

fn draw_keybar(frame: &mut Frame, area: Rect, app: &App) {
    let st = &app.theme.styles;
    frame.buffer_mut().set_style(area, st.status);

    let pairs: &[(&str, &str)] = if app.filter_editing {
        &[
            ("type", "filter"),
            ("⏎", "apply"),
            ("Esc", "clear"),
            ("↑↓", "move"),
        ]
    } else {
        &[
            ("↑↓", "move"),
            ("⏎", "open"),
            ("⌫", "up"),
            ("s", "size"),
            ("n", "name"),
            ("b", "bar"),
            ("a", "apparent"),
            ("/", "filter"),
            ("i", "info"),
            ("d", "delete"),
            ("r", "rescan"),
            ("T", "theme"),
            ("?", "help"),
            ("q", "quit"),
        ]
    };

    let mut spans: Vec<Span> = Vec::new();
    let mut used: usize = 0;
    for (key, label) in pairs {
        let k = format!(" {key} ");
        let l = format!(" {label}  ");
        let w = format::width(&k) + format::width(&l);
        if used + w > area.width as usize {
            break;
        }
        used += w;
        spans.push(Span::styled(k, st.keybar_key));
        spans.push(Span::styled(l, st.keybar_label));
    }
    frame.render_widget(Line::from(spans), area);
}
