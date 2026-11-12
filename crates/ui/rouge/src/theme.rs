use clankerdiff_ui_core::{
    Appearance, ButtonVariant, ControlState, NoticeTone, ReviewTheme, Rgba, SelectionState, SemanticStyle,
    TerminalPalette, ThemePreference, UiPalette,
};
use rouge_core::{Color, Palette, appearance, theme, use_theme};
use rouge_reactive::{Memo, ReadSignal, Signal, batch, expect_context, memo, provide_context, signal};

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
        let ui = UiTheme::provide(UiColors::from_palette(&review.ui));
        let active = Self { ui, review: signal(review), widgets };
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
            self.ui.set(UiColors::from_palette(&review.ui));
            self.review.set(review);
        });
    }

    #[must_use]
    pub fn review(self) -> ReviewTheme {
        self.review.get()
    }
    #[must_use]
    pub fn review_signal(self) -> ReadSignal<ReviewTheme> {
        self.review.read_only()
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

theme! {
    pub struct UiTheme from UiColors {
        canvas: Color,
        surface: Color,
        surface_hover: Color,
        surface_selected: Color,
        chrome: Color,
        elevated: Color,
        input: Color,
        text: Color,
        text_secondary: Color,
        text_muted: Color,
        border: Color,
        border_subtle: Color,
        accent: Color,
        accent_foreground: Color,
        info: Color,
        positive: Color,
        warning: Color,
        destructive: Color,
        destructive_foreground: Color,
        scrim: Color,
        semantic: UiPalette,
    }
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
            semantic: *palette,
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

impl UiTheme {
    pub fn paint(self, select: impl Fn(UiColors) -> Color + 'static) -> Memo<Color> {
        memo(move || select(self.get()))
    }
    #[must_use]
    pub fn control_style(self, variant: ButtonVariant, state: ControlState) -> UiStyle {
        self.semantic.with(|p| p.control_style(variant, state)).into()
    }
    #[must_use]
    pub fn selection_style(self, state: SelectionState) -> UiStyle {
        self.semantic.with(|p| p.selection_style(state)).into()
    }
    #[must_use]
    pub fn notice_style(self, tone: NoticeTone) -> UiStyle {
        self.semantic.with(|p| p.notice_style(tone)).into()
    }
}
