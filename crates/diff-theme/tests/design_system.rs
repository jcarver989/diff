use diff_theme::{
    ButtonVariant, ControlState, DiffPalette, InteractionState, NoticeTone, SCRIM_ALPHA,
    SelectionState, UiPalette,
};

#[test]
fn ui_palette_uses_stable_semantic_roles() {
    let diff = DiffPalette::default();
    let ui = UiPalette::from(&diff);
    assert_eq!(ui.canvas, diff.background);
    assert_eq!(ui.surface_selected, diff.selection);
    assert_eq!(ui.text_muted, diff.muted);
    assert_eq!(ui.positive, diff.addition);
    assert_eq!(ui.destructive, diff.deletion);
    assert_eq!(ui.scrim.a, SCRIM_ALPHA);
}

#[test]
fn semantic_states_resolve_before_renderer_conversion() {
    let ui = UiPalette::from(&DiffPalette::default());
    let primary = ui.control_style(
        ButtonVariant::Primary,
        ControlState::new(InteractionState::Rest),
    );
    assert_eq!(
        primary,
        ui.control_style(
            ButtonVariant::Primary,
            ControlState::new(InteractionState::Hovered)
        )
    );
    assert_eq!(primary.background, Some(ui.accent));
    assert_eq!(primary.foreground, ui.accent_foreground);

    let selected_hover = ui.control_style(
        ButtonVariant::Ghost,
        ControlState::new(InteractionState::Hovered).selected(true),
    );
    assert_eq!(selected_hover.background, Some(ui.surface_selected));
    assert_eq!(selected_hover.foreground, ui.accent);

    let focused = ui.selection_style(SelectionState::Focused);
    assert_eq!(focused.background, Some(ui.accent));
    assert!(focused.emphasized);

    let warning = ui.notice_style(NoticeTone::Warning);
    assert_eq!(warning.foreground, ui.accent);
    assert!(warning.emphasized);
}
