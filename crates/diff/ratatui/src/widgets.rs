use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget},
};

pub(crate) const SCROLLBAR_WIDTH: u16 = 1;

pub(crate) fn rows_and_track(area: Rect, scrollbar: bool) -> (Rect, Rect) {
    let track_width = if scrollbar { SCROLLBAR_WIDTH } else { 0 };
    let [rows, track] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(track_width.min(area.width)),
    ])
    .areas(area);
    (rows, track)
}

pub(crate) fn render_vertical_scrollbar(
    area: Rect,
    buffer: &mut Buffer,
    content_rows: usize,
    viewport_rows: usize,
    offset: usize,
) {
    if area.is_empty() {
        return;
    }
    let scrollable = content_rows.saturating_sub(viewport_rows);
    let mut state = ScrollbarState::new(scrollable).position(offset.min(scrollable));
    StatefulWidget::render(
        Scrollbar::new(ScrollbarOrientation::VerticalRight),
        area,
        buffer,
        &mut state,
    );
}
