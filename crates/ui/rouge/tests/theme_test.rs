use clankerdiff_rouge_ui::{
    prelude::*,
    testing::{TestResult, component_test},
};
use clankerdiff_ui_core::{Appearance, ReviewTheme, Rgba};
use rouge_core::{Styled, div};
use rouge_reactive::memo;

#[test]
fn mounted_palette_and_toolkit_theme_update_without_rebuilding_the_root() -> TestResult {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    let original = ReviewTheme::default();
    let next = ReviewTheme::builtin("ayu")?;
    let (mut ui, (active, appearance, widget_surface)) = component_test().build_with(|| {
        let active = ActiveTheme::install(original);
        let appearance = memo(window_appearance);
        let widget_surface = rouge_core::use_theme().surface;
        (div().size_full().bg(active.ui().paint(|c| c.elevated)), (active, appearance, widget_surface))
    });
    assert_eq!(appearance.get(), Appearance::Light);
    let before = ui.pixel(10, 10)?;
    active.set(next);
    ui.advance(0.0);
    assert_ne!(ui.pixel(10, 10)?, before);
    assert_eq!(widget_surface.get(), active.ui().colors().elevated);
    ui.set_appearance(rouge_core::Appearance::Dark);
    assert_eq!(appearance.get(), Appearance::Dark);
    Ok(())
}
#[test]
fn all_catalog_ui_roles_convert_without_diff_aliases() -> TestResult {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    for descriptor in ReviewTheme::catalog() {
        let mut theme = ReviewTheme::builtin(&descriptor.id)?;
        theme.diff.foreground = Rgba::new(1, 2, 3, 255);
        theme.diff.background = Rgba::new(4, 5, 6, 255);
        let native = UiTheme::new(&theme);
        let ui = theme.ui;
        for (actual, expected) in [
            (native.colors().canvas, ui.canvas),
            (native.colors().surface, ui.surface),
            (native.colors().surface_hover, ui.surface_hover),
            (native.colors().surface_selected, ui.surface_selected),
            (native.colors().text, ui.text),
            (native.colors().text_secondary, ui.text_secondary),
            (native.colors().text_muted, ui.text_muted),
            (native.colors().accent, ui.accent),
            (native.colors().accent_foreground, ui.accent_foreground),
            (native.colors().info, ui.info),
            (native.colors().positive, ui.positive),
            (native.colors().warning, ui.warning),
            (native.colors().destructive, ui.destructive),
            (native.colors().destructive_foreground, ui.destructive_foreground),
            (native.colors().border, ui.border),
            (native.colors().scrim, ui.scrim),
        ] {
            assert_eq!(actual, color(expected), "{}", descriptor.id);
        }
        assert_eq!(native.notice_style(NoticeTone::Info).foreground, color(ui.info));
        assert_eq!(native.notice_style(NoticeTone::Warning).foreground, color(ui.warning));
        assert_eq!(native.notice_style(NoticeTone::Neutral).foreground, color(ui.text_secondary));
    }
    Ok(())
}

#[test]
fn alpha_survives_ui_conversion_and_state_resolution() {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    let mut theme = ReviewTheme::default();
    theme.ui.accent = Rgba::new(220, 140, 100, 128);
    theme.ui.accent_foreground = Rgba::new(30, 40, 60, 96);
    theme.ui.info = Rgba::new(30, 180, 240, 64);
    let native = UiTheme::new(&theme);
    let style = native.control_style(ButtonVariant::Primary, ControlState::new(InteractionState::Rest));
    assert_eq!(style.background, Some(color(theme.ui.accent)));
    assert_eq!(style.foreground, color(theme.ui.accent_foreground));
    assert!((style.foreground.a - 96.0 / 255.0).abs() < f32::EPSILON);
    assert_eq!(native.notice_style(NoticeTone::Info).foreground, color(theme.ui.info));
    assert_eq!(native.selection_style(SelectionState::Focused).foreground, color(theme.ui.accent_foreground));
}
