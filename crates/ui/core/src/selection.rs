use crate::{Appearance, PickerOption, PickerState, ReviewTheme};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ThemeChoice {
    pub name: String,
    pub theme: ReviewTheme,
}

impl ThemeChoice {
    #[must_use]
    pub fn new(name: impl Into<String>, theme: ReviewTheme) -> Self {
        Self { name: name.into(), theme }
    }

    #[must_use]
    pub fn catalog() -> Arc<[Self]> {
        ReviewTheme::catalog()
            .into_iter()
            .filter_map(|descriptor| {
                ReviewTheme::builtin(&descriptor.id).ok().map(|theme| Self::new(descriptor.name, theme))
            })
            .collect()
    }

    #[must_use]
    pub fn catalog_for(appearance: Appearance) -> Arc<[Self]> {
        Self::catalog().iter().filter(|choice| choice.theme.appearance() == appearance).cloned().collect()
    }
}

#[derive(Debug)]
pub struct ThemeSelection {
    themes: Arc<[ThemeChoice]>,
    picker: PickerState,
    original: ReviewTheme,
}

impl ThemeSelection {
    #[must_use]
    pub fn new(current: &ReviewTheme, themes: Arc<[ThemeChoice]>) -> Option<Self> {
        if themes.is_empty() {
            return None;
        }
        let mut picker =
            PickerState::new(themes.iter().map(|choice| PickerOption::new(choice.name.clone())).collect()).unfiltered();
        if let Some(selected) = themes.iter().position(|choice| choice.theme.id() == current.id()) {
            picker.highlight(selected);
        }
        Some(Self { themes, picker, original: current.clone() })
    }

    #[must_use]
    pub fn themes(&self) -> &[ThemeChoice] {
        &self.themes
    }

    #[must_use]
    pub fn position(&self, id: &str) -> Option<usize> {
        self.themes.iter().position(|choice| choice.theme.id().to_string() == id)
    }

    #[must_use]
    pub const fn picker(&self) -> &PickerState {
        &self.picker
    }

    #[must_use]
    pub const fn selected(&self) -> usize {
        self.picker.highlighted_position()
    }

    #[must_use]
    pub fn selected_theme(&self) -> ReviewTheme {
        self.themes[self.selected()].theme.clone()
    }

    #[must_use]
    pub fn cancel(self) -> ReviewTheme {
        self.original
    }

    #[must_use]
    pub fn commit(self) -> ReviewTheme {
        self.selected_theme()
    }

    pub fn select_relative(&mut self, delta: isize) -> ReviewTheme {
        self.picker.move_by(delta);
        self.selected_theme()
    }

    pub fn select(&mut self, selected: usize) -> Option<ReviewTheme> {
        self.picker.highlight(selected).then(|| self.selected_theme())
    }
}
