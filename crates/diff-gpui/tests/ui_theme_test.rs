use clankerdiff_gpui::{style::color, ui::theme::UiTheme};
use clankerdiff_theme::{
    ButtonVariant, ControlState, InteractionState, NoticeTone, ReviewTheme, Rgba, SelectionState,
};
use std::error::Error;

#[test]
fn all_catalog_ui_roles_convert_without_diff_aliases() -> Result<(), Box<dyn Error>> {
    for descriptor in ReviewTheme::catalog() {
        let mut theme = ReviewTheme::builtin(&descriptor.id)?;
        theme.diff.foreground = Rgba::new(1, 2, 3, 255);
        theme.diff.background = Rgba::new(4, 5, 6, 255);
        let native = UiTheme::new(&theme);
        let ui = theme.ui;
        for (actual, expected) in [
            (native.colors.canvas, ui.canvas),
            (native.colors.surface, ui.surface),
            (native.colors.surface_hover, ui.surface_hover),
            (native.colors.surface_selected, ui.surface_selected),
            (native.colors.text, ui.text),
            (native.colors.text_secondary, ui.text_secondary),
            (native.colors.text_muted, ui.text_muted),
            (native.colors.accent, ui.accent),
            (native.colors.accent_foreground, ui.accent_foreground),
            (native.colors.info, ui.info),
            (native.colors.positive, ui.positive),
            (native.colors.warning, ui.warning),
            (native.colors.destructive, ui.destructive),
            (
                native.colors.destructive_foreground,
                ui.destructive_foreground,
            ),
            (native.colors.border, ui.border),
            (native.colors.scrim, ui.scrim),
        ] {
            assert_eq!(actual, color(expected), "{}", descriptor.id);
        }
        assert_eq!(
            native.notice_style(NoticeTone::Info).foreground,
            color(ui.info)
        );
        assert_eq!(
            native.notice_style(NoticeTone::Warning).foreground,
            color(ui.warning)
        );
        assert_eq!(
            native.notice_style(NoticeTone::Neutral).foreground,
            color(ui.text_secondary)
        );
    }
    Ok(())
}

#[test]
fn alpha_survives_ui_conversion_and_state_resolution() {
    let mut theme = ReviewTheme::default();
    theme.ui.accent = Rgba::new(220, 140, 100, 128);
    theme.ui.accent_foreground = Rgba::new(30, 40, 60, 96);
    theme.ui.info = Rgba::new(30, 180, 240, 64);
    let native = UiTheme::new(&theme);
    let style = native.control_style(
        ButtonVariant::Primary,
        ControlState::new(InteractionState::Rest),
    );
    assert_eq!(style.background, Some(color(theme.ui.accent)));
    assert_eq!(style.foreground, color(theme.ui.accent_foreground));
    assert!((style.foreground.a - 96.0 / 255.0).abs() < f32::EPSILON);
    assert_eq!(
        native.notice_style(NoticeTone::Info).foreground,
        color(theme.ui.info)
    );
    assert_eq!(
        native.selection_style(SelectionState::Focused).foreground,
        color(theme.ui.accent_foreground)
    );
}
