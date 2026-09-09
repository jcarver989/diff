use crate::ReviewTheme;
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
        ReviewTheme::catalog().into_iter().filter_map(|descriptor| {
            ReviewTheme::builtin(&descriptor.id).ok().map(|theme| Self::new(descriptor.name, theme))
        }).collect()
    }
}

#[derive(Debug)]
pub struct ThemeSelection {
    themes: Arc<[ThemeChoice]>,
    selected: usize,
    original: ReviewTheme,
}

impl ThemeSelection {
    #[must_use]
    pub fn new(current: &ReviewTheme, themes: Arc<[ThemeChoice]>) -> Option<Self> {
        if themes.is_empty() { return None; }
        let selected = themes.iter().position(|choice| choice.theme.id() == current.id()).unwrap_or(0);
        Some(Self { themes, selected, original: current.clone() })
    }

    #[must_use]
    pub fn themes(&self) -> &[ThemeChoice] { &self.themes }

    #[must_use]
    pub const fn selected(&self) -> usize { self.selected }

    #[must_use]
    pub fn selected_theme(&self) -> ReviewTheme { self.themes[self.selected].theme.clone() }

    #[must_use]
    pub fn cancel(self) -> ReviewTheme { self.original }

    #[must_use]
    pub fn commit(self) -> ReviewTheme { self.selected_theme() }

    pub fn select_relative(&mut self, delta: isize) -> ReviewTheme {
        self.selected = self.selected.saturating_add_signed(delta).min(self.themes.len() - 1);
        self.selected_theme()
    }

    pub fn select(&mut self, selected: usize) -> Option<ReviewTheme> {
        let theme = self.themes.get(selected)?.theme.clone();
        self.selected = selected;
        Some(theme)
    }
}
