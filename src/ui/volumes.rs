//! The volumes screen: every mounted and unmounted volume on the system.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};

use crate::app::App;
use crate::format;
use crate::theme;
use crate::volumes::{Volume, VolumeKind};

pub fn key_pairs() -> Vec<(&'static str, &'static str)> {
    vec![
        ("↑↓ jk", "move"),
        ("⏎ →", "scan volume"),
        ("r", "refresh"),
        ("Esc ← V", "back"),
        ("T", "themes"),
        ("?", "help"),
    ]
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let st = app.theme.styles.clone();
    let guide = super::browser::guide_lines(app, key_pairs(), area.width);
    let guide_h = guide
        .len()
        .min(area.height.saturating_sub(4).max(1) as usize) as u16;
    let [header, body, status, keybar] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(guide_h),
    ])
    .areas(area);

    // Header
    frame.buffer_mut().set_style(header, st.header);
    let title = if app.icons.emoji() {
        format!(" {} codu ", app.icons.app)
    } else {
        " codu ".to_string()
    };
    frame.render_widget(
        Line::from(vec![
            Span::styled(title, st.header_title),
            Span::styled(" Volumes ", st.header_path),
        ]),
        header,
    );
    let mood = match (app.icons.emoji(), app.theme.dark) {
        (false, _) => "",
        (true, Some(true)) => "🌙 ",
        (true, Some(false)) => "🌞 ",
        (true, None) => "🎨 ",
    };
    let theme_label = format!(" {mood}{} ", app.theme.name);
    frame.render_widget(
        Line::from(Span::styled(theme_label, st.header_theme)).right_aligned(),
        header,
    );

    // List
    let Some(view) = app.volumes.as_mut() else {
        return;
    };
    let n = view.list.len();
    let inner = if app.config.borders {
        let position = if n == 0 {
            String::new()
        } else {
            format!(" {}/{} ", view.cursor + 1, n)
        };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(st.border)
            .title_bottom(Line::from(position).style(st.border_title).right_aligned());
        let inner = block.inner(body);
        frame.render_widget(block, body);
        inner
    } else {
        body
    };
    app.list_area = inner;
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    view.ensure_visible(inner.height as usize);

    if n == 0 {
        let msg = if crate::volumes::supported() {
            "no volumes found"
        } else {
            "volume listing is not available on this platform"
        };
        frame.render_widget(
            Line::from(Span::styled(msg, st.hidden)).centered(),
            Rect::new(inner.x, inner.y + inner.height / 2, inner.width, 1),
        );
    }

    let cols = columns(inner.width);
    let icons = &app.icons;
    let bar_theme = app.theme.bar.clone();
    let end = (view.scroll + inner.height as usize).min(n);
    let cursor = view.cursor;
    let scroll = view.scroll;
    let rows: Vec<Volume> = view.list[scroll..end].to_vec();
    let buf = frame.buffer_mut();
    for (i, vol) in rows.iter().enumerate() {
        let rect = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        let selected = scroll + i == cursor;
        let sel = if selected { st.selected } else { Style::new() };
        if selected {
            buf.set_style(rect, st.selected);
        }
        let p = |s: Style| s.patch(sel);

        let icon = match (vol.mounted(), vol.kind) {
            (_, VolumeKind::Swap) => &icons.swap,
            (false, _) => &icons.unmounted,
            (true, VolumeKind::Removable) => &icons.removable,
            (true, VolumeKind::Network) => &icons.network,
            (true, VolumeKind::Optical) => &icons.optical,
            (true, VolumeKind::Disk) => &icons.volume,
        };
        let name_style = if vol.mounted() { st.dir } else { st.hidden };
        let mut spans: Vec<Span> = vec![Span::styled(
            if selected { "▸ " } else { "  " },
            p(st.marker),
        )];
        if icons.column_width() > 0 {
            spans.push(Span::styled(format!("{icon} "), p(name_style)));
        }
        spans.push(Span::styled(
            format::fit(&vol.name(), cols.name),
            p(name_style),
        ));

        if cols.device > 0 {
            let dev = if vol.mounted() {
                vol.short_device().to_string()
            } else {
                String::new()
            };
            spans.push(Span::styled(
                format!(" {}", format::fit(&dev, cols.device - 1)),
                p(st.count),
            ));
        }
        if cols.fs > 0 {
            let fs = if vol.fs_type.is_empty() {
                "?"
            } else {
                vol.fs_type.as_str()
            };
            spans.push(Span::styled(
                format!(" {}", format::fit(fs, cols.fs - 1)),
                p(st.mount),
            ));
        }
        if cols.label > 0 {
            let text = match (&vol.label, &vol.model) {
                (Some(l), _) => l.clone(),
                (None, Some(m)) => m.clone(),
                (None, None) => String::new(),
            };
            spans.push(Span::styled(
                format!(" {}", format::fit(&text, cols.label - 1)),
                p(st.hidden),
            ));
        }
        // used / bar / total / percent
        if vol.mounted() || vol.kind == VolumeKind::Swap {
            let (num, unit) = format::bytes(vol.used, app.si);
            spans.push(Span::styled(format!(" {num:>5} "), p(st.size)));
            spans.push(Span::styled(format!("{unit:<3}"), p(st.size_unit)));
        } else {
            spans.push(Span::styled(format!(" {:>9}", "not mounted"), p(st.hidden)));
        }
        if cols.bar > 0 {
            let width = cols.bar - 1;
            let filled = ((vol.share().clamp(0.0, 1.0)) * width as f64).round() as usize;
            let style = if bar_theme.gradient {
                Style::new().fg(theme::gradient(&bar_theme, vol.share()))
            } else {
                st.bar_filled
            };
            let (filled, empty) = if vol.mounted() || vol.kind == VolumeKind::Swap {
                (filled, width - filled)
            } else {
                (0, width)
            };
            spans.push(Span::raw(" "));
            spans.push(Span::styled(bar_theme.filled.repeat(filled), p(style)));
            spans.push(Span::styled(bar_theme.empty.repeat(empty), p(st.bar_empty)));
        }
        let (tnum, tunit) = format::bytes(vol.total, app.si);
        spans.push(Span::styled(format!(" {tnum:>5} "), p(st.count)));
        spans.push(Span::styled(format!("{tunit:<3}"), p(st.count)));
        if cols.percent > 0 {
            let pct = if vol.mounted() || vol.kind == VolumeKind::Swap {
                format::percent(vol.share())
            } else {
                "-".to_string()
            };
            spans.push(Span::styled(format!(" {pct:>6}"), p(st.percent)));
        }
        if cols.kind > 0 {
            let mut tag = vol.kind.label().to_string();
            if vol.read_only {
                tag.push_str(" ro");
            }
            spans.push(Span::styled(
                format!(" {}", format::fit(&tag, cols.kind - 1)),
                p(st.hidden),
            ));
        }
        ratatui::widgets::Widget::render(Line::from(spans), rect, buf);
    }

    // Status
    frame.buffer_mut().set_style(status, st.status);
    let mounted = app
        .volumes
        .as_ref()
        .map(|v| v.list.iter().filter(|x| x.mounted()).count())
        .unwrap_or(0);
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(if app.icons.emoji() { "💽 " } else { "" }, st.status),
        Span::styled(format::count(n as u64), st.status_accent),
        Span::styled(" volumes  ·  ", st.status),
        Span::styled(format::count(mounted as u64), st.status_accent),
        Span::styled(" mounted  ·  ⏎ scans a mounted volume", st.status),
    ];
    if app.tree.is_none() {
        spans.push(Span::styled("  ·  Esc quits", st.status));
    }
    let left_w: usize = spans.iter().map(|s| format::width(&s.content)).sum();
    frame.render_widget(Line::from(spans), status);
    if let Some(msg) = &app.status {
        let room = (status.width as usize).saturating_sub(left_w + 3);
        if room >= 8 {
            let style = if msg.warn { st.warning } else { st.success };
            let text = format!(" {} ", format::truncate_right(&msg.text, room));
            frame.render_widget(
                Line::from(Span::styled(text, style)).right_aligned(),
                status,
            );
        }
    }

    super::browser::draw_keybar(frame, keybar, app, guide);
}

#[derive(Clone, Copy, Default)]
struct VolCols {
    name: usize,
    device: usize,
    fs: usize,
    label: usize,
    bar: usize,
    percent: usize,
    kind: usize,
}

fn columns(width: u16) -> VolCols {
    let w = width as usize;
    let mut c = VolCols {
        device: 12,
        fs: 9,
        label: 18,
        bar: 13,
        percent: 7,
        kind: 11,
        ..VolCols::default()
    };
    // marker(2) + icon(3) + used(10) + total(10) are always present.
    let fixed = 2 + 3 + 10 + 10;
    let name_for = |c: &VolCols| {
        w as i64 - (fixed + c.device + c.fs + c.label + c.bar + c.percent + c.kind) as i64
    };
    if name_for(&c) < 16 {
        c.kind = 0;
    }
    if name_for(&c) < 16 {
        c.label = 0;
    }
    if name_for(&c) < 16 {
        c.device = 0;
    }
    if name_for(&c) < 16 {
        c.bar = 0;
    }
    if name_for(&c) < 16 {
        c.percent = 0;
    }
    if name_for(&c) < 16 {
        c.fs = 0;
    }
    c.name = name_for(&c).max(6) as usize;
    c
}
