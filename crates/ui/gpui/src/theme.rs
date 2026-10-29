//! GPUI-facing semantic theme tokens.

use clankerdiff_ui_core::{
    Appearance, ButtonVariant, ControlState, NoticeTone, ReviewTheme, Rgba, SelectionState, SemanticStyle,
    TerminalPalette, ThemePreference, UiPalette,
};
use gpui::{App, Global, Hsla, Window, WindowAppearance};
use gpui_base::input::InputEditorStyle;

/// Converts a renderer-neutral sRGB color to GPUI's HSLA color.
#[must_use]
pub fn color(value: Rgba) -> Hsla {
    gpui::rgba((u32::from(value.r) << 24) | (u32::from(value.g) << 16) | (u32::from(value.b) << 8) | u32::from(value.a))
        .into()
}

#[must_use]
pub fn window_appearance(window: &Window) -> Appearance {
    match window.appearance() {
        WindowAppearance::Light | WindowAppearance::VibrantLight => Appearance::Light,
        WindowAppearance::Dark | WindowAppearance::VibrantDark => Appearance::Dark,
    }
}

#[derive(Debug, Clone)]
pub struct ActiveTheme {
    review: ReviewTheme,
    ui: UiTheme,
    terminal: TerminalPalette,
}

impl Global for ActiveTheme {}

impl ActiveTheme {
    pub fn install(review: ReviewTheme, window: &mut Window, cx: &mut App) {
        cx.set_global(Self { ui: UiTheme::new(&review), terminal: review.terminal_palette(), review });
        window.refresh();
    }

    pub fn apply(preference: &ThemePreference, window: &mut Window, cx: &mut App) {
        Self::install(preference.theme(window_appearance(window)), window, cx);
    }

    #[must_use]
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    #[must_use]
    pub const fn review(&self) -> &ReviewTheme {
        &self.review
    }

    #[must_use]
    pub const fn ui(&self) -> UiTheme {
        self.ui
    }

    #[must_use]
    pub const fn terminal(&self) -> &TerminalPalette {
        &self.terminal
    }
}

/// Semantic colors consumed by reusable GPUI components.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiColors {
    pub canvas: Hsla,
    pub surface: Hsla,
    pub surface_hover: Hsla,
    pub surface_selected: Hsla,
    pub chrome: Hsla,
    pub elevated: Hsla,
    pub input: Hsla,
    pub text: Hsla,
    pub text_secondary: Hsla,
    pub text_muted: Hsla,
    pub border: Hsla,
    pub border_subtle: Hsla,
    pub accent: Hsla,
    pub accent_foreground: Hsla,
    pub info: Hsla,
    pub positive: Hsla,
    pub warning: Hsla,
    pub destructive: Hsla,
    pub destructive_foreground: Hsla,
    pub scrim: Hsla,
}

impl UiColors {
    #[must_use]
    pub fn from_palette(palette: &UiPalette) -> Self {
        Self {
            canvas: color(palette.canvas),
            surface: color(palette.surface),
            surface_hover: color(palette.surface_hover),
            surface_selected: color(palette.surface_selected),
            chrome: color(palette.chrome),
            elevated: color(palette.elevated),
            input: color(palette.input),
            text: color(palette.text),
            text_secondary: color(palette.text_secondary),
            text_muted: color(palette.text_muted),
            border: color(palette.border),
            border_subtle: color(palette.border_subtle),
            accent: color(palette.accent),
            accent_foreground: color(palette.accent_foreground),
            info: color(palette.info),
            positive: color(palette.positive),
            warning: color(palette.warning),
            destructive: color(palette.destructive),
            destructive_foreground: color(palette.destructive_foreground),
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

/// Semantic GPUI theme derived from a [`ReviewTheme`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiTheme {
    palette: UiPalette,
    pub colors: UiColors,
}

impl UiTheme {
    /// Creates component tokens from a diff theme.
    #[must_use]
    pub fn new(theme: &ReviewTheme) -> Self {
        let palette = theme.ui;
        Self { colors: UiColors::from_palette(&palette), palette }
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

    #[must_use]
    pub fn editor_style(self) -> InputEditorStyle {
        let colors = self.colors;
        let mut selection = colors.accent;
        selection.a = 0.3;
        InputEditorStyle {
            foreground: colors.text,
            muted_foreground: colors.text_muted,
            background: colors.surface,
            border: colors.border,
            selection,
            caret: colors.text,
            ..InputEditorStyle::default()
        }
    }
}
