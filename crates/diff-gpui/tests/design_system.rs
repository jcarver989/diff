use diff_gpui::ui::{theme::UiTheme, tokens};
use diff_theme::{
    ButtonVariant, ControlState, DiffTheme, InteractionState, NoticeTone, SelectionState,
};

#[test]
fn semantic_theme_maps_the_diff_palette() {
    let source = DiffTheme::default();
    let ui = UiTheme::new(&source);
    assert_eq!(
        ui.colors.canvas,
        diff_gpui::style::color(source.palette().background)
    );
    assert_eq!(
        ui.colors.accent,
        diff_gpui::style::color(source.palette().accent)
    );
    assert_eq!(
        ui.colors.destructive,
        diff_gpui::style::color(source.palette().deletion)
    );
    assert!((ui.colors.scrim.a - tokens::SCRIM_OPACITY).abs() < f32::EPSILON);
}

#[test]
fn adapter_resolves_complete_component_states() {
    let theme = UiTheme::new(&DiffTheme::default());
    let primary = theme.control_style(
        ButtonVariant::Primary,
        ControlState::new(InteractionState::Rest),
    );
    let hovered = theme.control_style(
        ButtonVariant::Primary,
        ControlState::new(InteractionState::Hovered),
    );
    assert_eq!(primary, hovered);
    assert_eq!(primary.background, Some(theme.colors.accent));
    assert_eq!(primary.foreground, theme.colors.accent_foreground);

    let selected = theme.selection_style(SelectionState::Focused);
    assert_eq!(selected.background, Some(theme.colors.accent));
    assert!(selected.emphasized);

    let error = theme.notice_style(NoticeTone::Error);
    assert_eq!(error.foreground, theme.colors.destructive);
}
