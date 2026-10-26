use clankerdiff_core::DiffTone;
use clankerdiff_ratatui::{RatatuiTheme, RatatuiUiTheme, composite_color, layered_style};
use clankerdiff_theme::{
    ButtonVariant, ControlState, InteractionState, NoticeTone, ReviewTheme, Rgba, SelectionState,
};
use ratatui::style::{Color, Modifier};
use std::error::Error;

#[test]
fn all_catalog_roles_convert_from_ui_not_diff() -> Result<(), Box<dyn Error>> {
    for descriptor in ReviewTheme::catalog() {
        let mut theme = ReviewTheme::builtin(&descriptor.id)?;
        theme.diff.background = Rgba::new(1, 2, 3, 255);
        theme.diff.foreground = Rgba::new(4, 5, 6, 255);
        let native = RatatuiTheme::from(&theme);
        let ui = theme.ui;
        for (actual, expected) in [
            (native.ui.canvas, ui.canvas),
            (native.ui.surface, ui.surface),
            (native.ui.surface_hover, ui.surface_hover),
            (native.ui.surface_selected, ui.surface_selected),
            (native.ui.text, ui.text),
            (native.ui.text_secondary, ui.text_secondary),
            (native.ui.text_muted, ui.text_muted),
            (native.ui.info, ui.info),
            (native.ui.warning, ui.warning),
            (native.ui.positive, ui.positive),
            (native.ui.destructive, ui.destructive),
            (native.ui.border, ui.border),
            (native.ui.accent, ui.accent),
        ] {
            assert_eq!(
                actual,
                composite_color(expected, ui.canvas),
                "{}",
                descriptor.id
            );
        }
        assert_eq!(
            native.tone(DiffTone::Context),
            (Color::Rgb(4, 5, 6), Color::Rgb(1, 2, 3))
        );
        assert_eq!(
            native.addition,
            composite_color(theme.diff.addition, theme.diff.background)
        );
    }
    Ok(())
}

#[test]
fn translucent_semantic_styles_composite_against_their_actual_surface() {
    let mut palette = ReviewTheme::default().ui;
    palette.canvas = Rgba::new(20, 30, 40, 128);
    palette.surface = Rgba::new(80, 100, 120, 128);
    palette.accent = Rgba::new(220, 140, 100, 128);
    palette.accent_foreground = Rgba::new(30, 40, 60, 128);
    palette.destructive = Rgba::new(240, 10, 40, 128);
    palette.destructive_foreground = Rgba::new(240, 230, 220, 128);
    palette.info = Rgba::new(30, 180, 240, 128);
    let native = RatatuiUiTheme::from(&palette);
    let canvas = palette.canvas.over(Rgba::new(0, 0, 0, 255));
    assert_eq!(
        native.canvas,
        composite_color(palette.canvas, Rgba::new(0, 0, 0, 255))
    );
    let rest = ControlState::new(InteractionState::Rest);
    assert_eq!(
        native.control_style(ButtonVariant::Primary, rest),
        layered_style(palette.accent_foreground, palette.accent, palette.canvas)
    );
    assert_eq!(
        native.control_style(ButtonVariant::Destructive, rest),
        layered_style(
            palette.destructive_foreground,
            palette.destructive,
            palette.canvas
        )
    );
    assert_eq!(
        Some(native.accent_foreground),
        native.control_style(ButtonVariant::Primary, rest).fg
    );
    let on_surface = native.on_background(native.surface);
    let surface = palette.surface.over(canvas);
    assert_eq!(
        on_surface.notice_style(NoticeTone::Info).fg,
        Some(composite_color(palette.info, surface))
    );
    assert_eq!(
        on_surface.control_style(ButtonVariant::Primary, rest),
        layered_style(palette.accent_foreground, palette.accent, surface)
    );
    assert_eq!(
        on_surface.selection_style(SelectionState::Focused),
        layered_style(palette.accent_foreground, palette.accent, surface)
            .add_modifier(Modifier::BOLD)
    );
    assert_eq!(on_surface.on_background(native.canvas), native);
    assert_eq!(
        native.selection_style(SelectionState::Focused).bg,
        Some(composite_color(palette.accent, palette.canvas))
    );
}
