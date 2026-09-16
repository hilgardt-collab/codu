//! The "scanning…" screen shown while the background scan runs.

use std::sync::atomic::Ordering;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

use crate::app::App;
use crate::format;
use crate::ui::centered;

pub fn draw(frame: &mut Frame, app: &App) {
    let Some(job) = &app.scan else { return };
    let st = &app.theme.styles;
    let icons = &app.icons;
    let area = frame.area();
    let width = 76.min(area.width.saturating_sub(2)).max(20);
    let rect = centered(area, width, 9);

    frame.render_widget(Clear, rect);
    let title = match (job.target.is_empty(), app.scan_options.one_file_system) {
        (true, true) => " Scanning (this volume only) ",
        (true, false) => " Scanning (across all volumes) ",
        (false, _) => " Rescanning ",
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(st.popup_border)
        .style(st.popup)
        .title(Line::from(title).style(st.popup_title));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let [_, l1, _, l2, _, l3, l4] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    let spinner = &icons.spinner[(app.tick as usize / 2) % icons.spinner.len().max(1)];
    let path = job.path.display().to_string();
    let path_w = inner
        .width
        .saturating_sub(4 + format::width(spinner) as u16) as usize;
    frame.render_widget(
        Line::from(vec![
            Span::raw("  "),
            Span::styled(spinner.clone(), st.spinner),
            Span::raw(" "),
            Span::styled(format::truncate_left(&path, path_w), st.scan_path),
        ]),
        l1,
    );

    let items = job.progress.items.load(Ordering::Relaxed);
    let bytes = job.progress.bytes.load(Ordering::Relaxed);
    let elapsed = job.started.elapsed();
    let (i_items, i_bytes, i_time) = if icons.emoji() {
        ("📄 ", "💾 ", "⌛ ")
    } else {
        ("", "", "")
    };
    frame.render_widget(
        Line::from(vec![
            Span::raw("  "),
            Span::styled(i_items, st.scan_label),
            Span::styled(format::count(items), st.scan_value),
            Span::styled(" items   ", st.scan_label),
            Span::styled(i_bytes, st.scan_label),
            Span::styled(format::bytes_str(bytes, app.si), st.scan_value),
            Span::styled("   ", st.scan_label),
            Span::styled(i_time, st.scan_label),
            Span::styled(format::duration(elapsed), st.scan_value),
        ]),
        l2,
    );

    let current = job.progress.current_path();
    let cur_w = inner.width.saturating_sub(5) as usize;
    frame.render_widget(
        Line::from(vec![
            Span::raw("  "),
            Span::styled(if icons.emoji() { "📂 " } else { "" }, st.scan_label),
            Span::styled(format::truncate_left(&current, cur_w), st.scan_path),
        ]),
        l3,
    );

    let hint = if job.target.is_empty() {
        " q "
    } else {
        " Esc "
    };
    let label = if job.target.is_empty() {
        " abort"
    } else {
        " cancel rescan"
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(hint, st.keybar_key),
            Span::styled(label, st.keybar_label),
        ]))
        .centered(),
        l4,
    );
}
