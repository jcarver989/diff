use crate::{KeyCode, KeyEvent};
use crate::{
    RatatuiTheme,
    ui::{Modal, ModalSize},
};
use clankerdiff_theme::{ReviewTheme, SelectionState, ThemeDescriptor};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{List, ListItem, ListState, StatefulWidget},
};

#[derive(Debug)]
pub(crate) struct ThemePicker {
    themes: Vec<ThemeDescriptor>,
    selected: usize,
    original: ReviewTheme,
}

pub(crate) enum ThemePickerAction {
    Preview(ReviewTheme),
    Restore(ReviewTheme),
    Commit,
    None,
}

impl ThemePicker {
    pub(crate) fn new(current: &ReviewTheme) -> Self {
        let themes = ReviewTheme::catalog();
        let current_id = current.id().to_string();
        let selected = themes
            .iter()
            .position(|theme| theme.id == current_id)
            .unwrap_or(0);
        Self {
            themes,
            selected,
            original: current.clone(),
        }
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> ThemePickerAction {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => ThemePickerAction::Restore(self.original.clone()),
            KeyCode::Enter => ThemePickerAction::Commit,
            KeyCode::Up | KeyCode::Char('k') => self.select_relative(-1),
            KeyCode::Down | KeyCode::Char('j') => self.select_relative(1),
            KeyCode::Home | KeyCode::Char('g') => self.select(0),
            KeyCode::End | KeyCode::Char('G') => self.select(self.themes.len().saturating_sub(1)),
            _ => ThemePickerAction::None,
        }
    }

    fn select_relative(&mut self, delta: isize) -> ThemePickerAction {
        let last = self.themes.len().saturating_sub(1);
        let selected = self.selected.saturating_add_signed(delta).min(last);
        self.select(selected)
    }

    fn select(&mut self, selected: usize) -> ThemePickerAction {
        self.selected = selected;
        ReviewTheme::builtin(&self.themes[self.selected].id)
            .map_or(ThemePickerAction::None, ThemePickerAction::Preview)
    }
}

/// Routes a key to an open picker. Returns a theme to apply when the picker
/// previews or restores one, and closes the picker on commit or restore.
pub(crate) fn apply_key(picker: &mut Option<ThemePicker>, key: KeyEvent) -> Option<ReviewTheme> {
    let open = picker.as_mut()?;
    match open.handle_key(key) {
        ThemePickerAction::Preview(theme) => Some(theme),
        ThemePickerAction::Restore(theme) => {
            *picker = None;
            Some(theme)
        }
        ThemePickerAction::Commit => {
            *picker = None;
            None
        }
        ThemePickerAction::None => None,
    }
}

pub(crate) fn render_theme_picker(
    area: Rect,
    buffer: &mut Buffer,
    picker: &ThemePicker,
    theme: &RatatuiTheme,
) {
    let popup = Modal::new("Theme", ModalSize::Medium, theme)
        .hint("j/k preview · Enter save · Esc cancel")
        .render(area, buffer);
    if popup.is_empty() {
        return;
    }
    let items = picker.themes.iter().map(|descriptor| {
        let appearance = if descriptor.is_dark { "dark" } else { "light" };
        ListItem::new(Line::from(vec![
            Span::raw(descriptor.name.clone()),
            Span::styled(
                format!("  {appearance}"),
                Style::new().fg(theme.ui.text_muted),
            ),
        ]))
    });
    let mut state = ListState::default().with_selected(Some(picker.selected));
    let list = List::new(items)
        .style(theme.ui.selection_style(SelectionState::None))
        .highlight_style(theme.ui.selection_style(SelectionState::Selected))
        .highlight_symbol("› ");
    StatefulWidget::render(list, popup, buffer, &mut state);
}
