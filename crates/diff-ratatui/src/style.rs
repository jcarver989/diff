//! Ratatui conversions for renderer-neutral diff themes.

use diff_core::{DiffTheme, DiffTone, FontStyle, Rgba};
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

impl RatatuiTheme {
    #[must_use]
    pub const fn tone(&self, tone: DiffTone) -> (Color, Color) {
        match tone {
            DiffTone::Added => (self.addition, self.addition_background),
            DiffTone::Removed => (self.deletion, self.deletion_background),
            DiffTone::Context | DiffTone::Meta => (self.foreground, self.background),
        }
    }
}

impl From<&DiffTheme> for RatatuiTheme {
    fn from(theme: &DiffTheme) -> Self {
        let palette = theme.palette();
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
    modifiers.set(Modifier::BOLD, font.bold);
    modifiers.set(Modifier::ITALIC, font.italic);
    modifiers.set(Modifier::UNDERLINED, font.underline);
    Style::new()
        .fg(color(foreground))
        .bg(background)
        .add_modifier(modifiers)
}
