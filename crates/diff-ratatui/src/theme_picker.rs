use crate::{RatatuiTheme, ui::{Modal, ModalSize}};
use clankerdiff_theme::SelectionState;
pub use clankerdiff_theme::ThemeChoice;
pub(crate) use clankerdiff_theme::ThemeSelection as ThemePicker;
use ratatui::{buffer::Buffer, layout::Rect, widgets::{List, ListItem, ListState, StatefulWidget}};

pub(crate) fn render_theme_picker(area: Rect, buffer: &mut Buffer, picker: &ThemePicker, theme: &RatatuiTheme) {
    let popup = Modal::new("Theme", ModalSize::Medium, theme)
        .hint("j/k preview · Enter save · Esc cancel")
        .render(area, buffer);
    if popup.is_empty() { return; }
    let items = picker.themes().iter().map(|choice| ListItem::new(choice.name.clone()));
    let mut state = ListState::default().with_selected(Some(picker.selected()));
    let list = List::new(items)
        .style(theme.ui.selection_style(SelectionState::None))
        .highlight_style(theme.ui.selection_style(SelectionState::Selected))
        .highlight_symbol("› ");
    StatefulWidget::render(list, popup, buffer, &mut state);
}
