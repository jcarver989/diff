//! Ratatui conversions for renderer-neutral diff themes.

use diff_core::{DiffTheme, FontStyle, Rgba};
use ratatui::style::{Color, Modifier, Style};

/// Ratatui colors derived from a shared [`DiffTheme`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RatatuiTheme {
    /// Main background.
    pub background: Color,
    /// Main foreground.
    pub foreground: Color,
    /// Line-number and secondary text.
    pub gutter: Color,
    /// Added-line foreground.
    pub addition: Color,
    /// Removed-line foreground.
    pub deletion: Color,
    /// Added-line background.
    pub addition_background: Color,
    /// Removed-line background.
    pub deletion_background: Color,
    /// Selected-row background.
    pub selection: Color,
    /// Focus and action accent.
    pub accent: Color,
    /// Muted metadata.
    pub muted: Color,
    /// Borders and separators.
    pub border: Color,
}

impl From<&DiffTheme> for RatatuiTheme {
    fn from(theme: &DiffTheme) -> Self {
        let palette = &theme.palette;
        Self {
            background: color(palette.background),
            foreground: color(palette.foreground),
            gutter: color(palette.gutter),
            addition: color(palette.addition),
            deletion: color(palette.deletion),
            addition_background: color(palette.addition_background),
            deletion_background: color(palette.deletion_background),
            selection: color(palette.selection),
            accent: color(palette.accent),
            muted: color(palette.muted),
            border: color(palette.border),
        }
    }
}

pub(crate) const fn color(value: Rgba) -> Color {
    Color::Rgb(value.r, value.g, value.b)
}

pub(crate) fn syntax_style(foreground: Rgba, font: FontStyle, background: Color) -> Style {
    let mut modifiers = Modifier::empty();
    if font.contains(FontStyle::BOLD) {
        modifiers.insert(Modifier::BOLD);
    }
    if font.contains(FontStyle::ITALIC) {
        modifiers.insert(Modifier::ITALIC);
    }
    if font.contains(FontStyle::UNDERLINE) {
        modifiers.insert(Modifier::UNDERLINED);
    }
    Style::new()
        .fg(color(foreground))
        .bg(background)
        .add_modifier(modifiers)
}
