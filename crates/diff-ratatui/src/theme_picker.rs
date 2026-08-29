use crate::RatatuiTheme;
use crossterm::event::{KeyCode, KeyEvent};
use diff_core::{DiffTheme, ThemeDescriptor};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Clear, List, ListItem, ListState, StatefulWidget, Widget},
};

#[derive(Debug)]
pub(crate) struct ThemePicker {
    themes: Vec<ThemeDescriptor>,
    selected: usize,
    original: DiffTheme,
}

pub(crate) enum ThemePickerAction {
    Preview(DiffTheme),
    Restore(DiffTheme),
    Commit,
    None,
}

impl ThemePicker {
    pub(crate) fn new(current: &DiffTheme) -> Self {
        let themes = DiffTheme::catalog();
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
        DiffTheme::builtin(&self.themes[self.selected].id)
            .map_or(ThemePickerAction::None, ThemePickerAction::Preview)
    }
}

pub(crate) fn render_theme_picker(
    area: Rect,
    buffer: &mut Buffer,
    picker: &ThemePicker,
    theme: &RatatuiTheme,
) {
    let width = area.width.saturating_sub(4).min(48);
    let height = area.height.saturating_sub(4).min(22);
    if width == 0 || height == 0 {
        return;
    }
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    Clear.render(popup, buffer);
    let items = picker.themes.iter().map(|descriptor| {
        let appearance = if descriptor.is_dark { "dark" } else { "light" };
        ListItem::new(Line::from(vec![
            Span::raw(descriptor.name.clone()),
            Span::styled(format!("  {appearance}"), Style::new().fg(theme.muted)),
        ]))
    });
    let mut state = ListState::default().with_selected(Some(picker.selected));
    let list = List::new(items)
        .block(
            Block::bordered()
                .title(" Theme ")
                .title_bottom(Line::from(" j/k preview · Enter save · Esc cancel ")),
        )
        .style(Style::new().fg(theme.foreground).bg(theme.background))
        .highlight_style(Style::new().fg(theme.accent).bg(theme.selection))
        .highlight_symbol("› ");
    StatefulWidget::render(list, popup, buffer, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use diff_core::ThemeId;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn previews_navigation_and_restores_original_theme() {
        let original = DiffTheme::default();
        let mut picker = ThemePicker::new(&original);
        let ThemePickerAction::Preview(preview) = picker.handle_key(key(KeyCode::Down)) else {
            panic!("down should preview a theme");
        };
        assert_ne!(preview.id(), &ThemeId::Sage);

        let ThemePickerAction::Restore(restored) = picker.handle_key(key(KeyCode::Esc)) else {
            panic!("escape should restore the opening theme");
        };
        assert_eq!(restored.id(), &ThemeId::Sage);
    }

    #[test]
    fn enter_commits_selection() {
        let mut picker = ThemePicker::new(&DiffTheme::default());
        assert!(matches!(
            picker.handle_key(key(KeyCode::Enter)),
            ThemePickerAction::Commit
        ));
    }
}
