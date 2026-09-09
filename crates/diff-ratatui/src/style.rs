//! Ratatui conversions for renderer-neutral diff themes.

use crate::color::native_color;
use crate::color::{composite_color, layered_style};
use clankerdiff_core::DiffTone;
use clankerdiff_theme::FontStyle;
use clankerdiff_theme::{
    ButtonVariant, ControlState, ModalSize, NoticeTone, ReviewTheme, Rgba, SelectionState,
    SemanticStyle, UiPalette,
};
use ratatui::style::{Color, Modifier, Style};

/// Ratatui adapter for shared semantic component states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RatatuiUiTheme {
    palette: UiPalette,
    pub canvas: Color,
    pub surface: Color,
    pub surface_hover: Color,
    pub surface_selected: Color,
    pub text: Color,
    pub text_muted: Color,
    pub border: Color,
    pub accent: Color,
    pub accent_foreground: Color,
    pub positive: Color,
    pub destructive: Color,
}

impl RatatuiUiTheme {
    #[must_use]
    pub fn control_style(self, variant: ButtonVariant, state: ControlState) -> Style {
        semantic_style(
            self.palette.control_style(variant, state),
            self.palette.canvas,
        )
    }

    #[must_use]
    pub fn selection_style(self, state: SelectionState) -> Style {
        semantic_style(self.palette.selection_style(state), self.palette.canvas)
    }

    #[must_use]
    pub fn notice_style(self, tone: NoticeTone) -> Style {
        semantic_style(self.palette.notice_style(tone), self.palette.canvas)
    }

    #[must_use]
    pub const fn modal_size(size: ModalSize) -> (u16, u16) {
        match size {
            ModalSize::Compact => (40, 12),
            ModalSize::Medium => (58, 18),
            ModalSize::Wide => (72, 22),
        }
    }
}

impl From<&UiPalette> for RatatuiUiTheme {
    fn from(palette: &UiPalette) -> Self {
        let color = |value| composite_color(value, palette.canvas);
        Self {
            palette: *palette,
            canvas: color(palette.canvas),
            surface: color(palette.surface),
            surface_hover: color(palette.surface_hover),
            surface_selected: color(palette.surface_selected),
            text: color(palette.text),
            text_muted: color(palette.text_muted),
            border: color(palette.border),
            accent: color(palette.accent),
            accent_foreground: color(palette.accent_foreground),
            positive: color(palette.positive),
            destructive: color(palette.destructive),
        }
    }
}

fn semantic_style(style: SemanticStyle, canvas: Rgba) -> Style {
    let mut native = match style.background {
        Some(background) => layered_style(style.foreground, background, canvas),
        None => Style::new().fg(composite_color(style.foreground, canvas)),
    };
    if style.emphasized {
        native = native.add_modifier(Modifier::BOLD);
    }
    native
}

/// Ratatui colors derived from a shared [`ReviewTheme`]. Application colors live
/// in [`RatatuiUiTheme`]; only diff-specific colors are kept here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RatatuiTheme {
    /// Semantic application colors.
    pub ui: RatatuiUiTheme,
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
}

impl RatatuiTheme {
    #[must_use]
    pub const fn tone(&self, tone: DiffTone) -> (Color, Color) {
        match tone {
            DiffTone::Added => (self.addition, self.addition_background),
            DiffTone::Removed => (self.deletion, self.deletion_background),
            DiffTone::Context | DiffTone::Meta => (self.ui.text, self.ui.canvas),
        }
    }
}

impl From<&ReviewTheme> for RatatuiTheme {
    fn from(theme: &ReviewTheme) -> Self {
        let palette = &theme.diff;
        let color = |value| composite_color(value, palette.background);
        Self {
            ui: RatatuiUiTheme::from(&UiPalette::from(palette)),
            gutter: color(palette.gutter),
            addition: color(palette.addition),
            deletion: color(palette.deletion),
            addition_background: color(palette.addition_background),
            deletion_background: color(palette.deletion_background),
        }
    }
}

pub(crate) fn syntax_style(foreground: Rgba, font: FontStyle, background: Color) -> Style {
    let mut modifiers = Modifier::empty();
    modifiers.set(Modifier::BOLD, font.bold);
    modifiers.set(Modifier::ITALIC, font.italic);
    modifiers.set(Modifier::UNDERLINED, font.underline);
    Style::new()
        .fg(native_color(foreground, background))
        .bg(background)
        .add_modifier(modifiers)
}
