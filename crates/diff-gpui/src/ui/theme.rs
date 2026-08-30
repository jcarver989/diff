//! GPUI-facing semantic theme tokens.

use crate::style::color;
use diff_theme::{
    ButtonVariant, ControlState, DiffTheme, NoticeTone, SelectionState, SemanticStyle, UiPalette,
};
use gpui::Hsla;

/// Semantic colors consumed by reusable GPUI components.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiColors {
    pub canvas: Hsla,
    pub surface: Hsla,
    pub surface_hover: Hsla,
    pub surface_selected: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub border: Hsla,
    pub accent: Hsla,
    pub accent_foreground: Hsla,
    pub positive: Hsla,
    pub destructive: Hsla,
    pub scrim: Hsla,
}

impl UiColors {
    /// Maps the renderer-neutral diff palette to UI roles.
    #[must_use]
    pub fn from_palette(palette: &UiPalette) -> Self {
        Self {
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
            scrim: color(palette.scrim),
        }
    }
}

/// GPUI-native result of resolving a semantic component state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiStyle {
    pub foreground: Hsla,
    pub background: Option<Hsla>,
    pub emphasized: bool,
}

impl From<SemanticStyle> for UiStyle {
    fn from(style: SemanticStyle) -> Self {
        Self {
            foreground: color(style.foreground),
            background: style.background.map(color),
            emphasized: style.emphasized,
        }
    }
}

/// Semantic GPUI theme derived from a [`DiffTheme`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiTheme {
    palette: UiPalette,
    pub colors: UiColors,
}

impl UiTheme {
    /// Creates component tokens from a diff theme.
    #[must_use]
    pub fn new(theme: &DiffTheme) -> Self {
        let palette = UiPalette::from(theme.palette());
        Self {
            colors: UiColors::from_palette(&palette),
            palette,
        }
    }

    #[must_use]
    pub fn control_style(self, variant: ButtonVariant, state: ControlState) -> UiStyle {
        self.palette.control_style(variant, state).into()
    }

    #[must_use]
    pub fn selection_style(self, state: SelectionState) -> UiStyle {
        self.palette.selection_style(state).into()
    }

    #[must_use]
    pub fn notice_style(self, tone: NoticeTone) -> UiStyle {
        self.palette.notice_style(tone).into()
    }
}
