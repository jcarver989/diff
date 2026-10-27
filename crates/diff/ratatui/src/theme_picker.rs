use crate::{
    RatatuiTheme,
    ui::{Modal, ModalList, ModalSize},
};
pub use clankerdiff_theme::ThemeChoice;
pub(crate) use clankerdiff_theme::ThemeSelection as ThemePicker;
use ratatui::{buffer::Buffer, layout::Rect, text::Line};

pub(crate) fn render_theme_picker(area: Rect, buffer: &mut Buffer, picker: &ThemePicker, theme: &RatatuiTheme) {
    let modal = Modal::new("Theme", ModalSize::Medium, theme).hint("j/k preview · Enter save · Esc cancel");
    ModalList::new(modal, picker.selected(), theme)
        .render(area, buffer, |_| picker.themes().iter().map(|choice| Line::from(choice.name.clone())).collect());
}
