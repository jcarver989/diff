use crate::{
    RatatuiTheme,
    ui::{Modal, ModalSize, render_picker},
};
pub use clankerdiff_ui_core::ThemeChoice;
pub(crate) use clankerdiff_ui_core::ThemeSelection as ThemePicker;
use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    text::Line,
};

pub(crate) fn render_theme_picker(
    area: Rect,
    buffer: &mut Buffer,
    picker: &ThemePicker,
    theme: &RatatuiTheme,
    pointer: Option<Position>,
) -> Rect {
    let content = Modal::new("Theme", ModalSize::Medium, theme)
        .hint("j/k preview · Enter save · Esc cancel")
        .render(area, buffer);
    render_picker(content, buffer, theme, picker.picker(), None, pointer, |_| {
        picker.themes().iter().map(|choice| Line::from(choice.name.clone())).collect()
    });
    content
}
