use clankerdiff_ui_core::{
    Appearance, ButtonVariant, ControlState, NoticeTone, ReviewTheme, Rgba, SelectionState, SemanticStyle,
    TerminalPalette, ThemePreference, UiPalette,
};
use rouge_core::{Color, Palette, appearance, use_theme};
use rouge_reactive::{Memo, Signal, batch, expect_context, memo, provide_context, signal};

#[must_use]
pub fn color(value: Rgba) -> Color {
    Color::new(
        f32::from(value.r) / 255.0,
        f32::from(value.g) / 255.0,
        f32::from(value.b) / 255.0,
        f32::from(value.a) / 255.0,
    )
}

#[must_use]
pub fn window_appearance() -> Appearance {
    match appearance().get() {
        rouge_core::Appearance::Light => Appearance::Light,
        rouge_core::Appearance::Dark => Appearance::Dark,
    }
}

#[derive(Clone, Copy)]
pub struct ActiveTheme {
    review: Signal<ReviewTheme>,
    ui: UiTheme,
    widgets: rouge_core::Theme,
}

impl ActiveTheme {
    #[must_use]
    pub fn install(review: ReviewTheme) -> Self {
        let widgets = use_theme();
        widgets.set(widget_palette(&review.ui));
        let active = Self { ui: UiTheme::new(&review), review: signal(review), widgets };
        provide_context(active);
        active
    }

    #[must_use]
    pub fn current() -> Self {
        expect_context()
    }

    pub fn apply(self, preference: &ThemePreference) {
        self.set(preference.theme(window_appearance()));
    }

    pub fn set(self, review: ReviewTheme) {
        if self.review.with_untracked(|old| old.revision() == review.revision()) {
            return;
        }
        batch(|| {
            self.widgets.set(widget_palette(&review.ui));
            self.ui.palette.set_if_changed(review.ui);
            self.review.set(review);
        });
    }

    #[must_use]
    pub fn review(self) -> ReviewTheme {
        self.review.get()
    }
    #[must_use]
    pub const fn ui(self) -> UiTheme {
        self.ui
    }
    pub fn terminal(self) -> TerminalPalette {
        self.review.with(ReviewTheme::terminal_palette)
    }
}

fn widget_palette(palette: &UiPalette) -> Palette {
    let c = UiColors::from_palette(palette);
    Palette {
        background: c.canvas,
        surface: c.elevated,
        text: c.text,
        text_muted: c.text_muted,
        accent: c.accent,
        accent_text: c.accent_foreground,
        border: c.border,
        hover: c.surface_hover,
        pressed: c.surface_selected,
        focus_ring: c.accent,
        danger: c.destructive,
        scrim: c.scrim,
        ..Palette::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiColors {
    pub canvas: Color,
    pub surface: Color,
    pub surface_hover: Color,
    pub surface_selected: Color,
    pub chrome: Color,
    pub elevated: Color,
    pub input: Color,
    pub text: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub border: Color,
    pub border_subtle: Color,
    pub accent: Color,
    pub accent_foreground: Color,
    pub info: Color,
    pub positive: Color,
    pub warning: Color,
    pub destructive: Color,
    pub destructive_foreground: Color,
    pub scrim: Color,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiStyle {
    pub foreground: Color,
    pub background: Option<Color>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiTheme {
    palette: Signal<UiPalette>,
}

impl UiTheme {
    #[must_use]
    pub fn new(theme: &ReviewTheme) -> Self {
        Self { palette: signal(theme.ui) }
    }
    pub fn colors(self) -> UiColors {
        self.palette.with(UiColors::from_palette)
    }
    pub fn paint(self, select: impl Fn(UiColors) -> Color + 'static) -> Memo<Color> {
        memo(move || select(self.colors()))
    }
    #[must_use]
    pub fn control_style(self, variant: ButtonVariant, state: ControlState) -> UiStyle {
        self.palette.with(|p| p.control_style(variant, state)).into()
    }
    #[must_use]
    pub fn selection_style(self, state: SelectionState) -> UiStyle {
        self.palette.with(|p| p.selection_style(state)).into()
    }
    #[must_use]
    pub fn notice_style(self, tone: NoticeTone) -> UiStyle {
        self.palette.with(|p| p.notice_style(tone)).into()
    }
}
