use clankerdiff_theme::{
    ButtonVariant, ControlState, InteractionState, NoticeTone, ReviewTheme, SCRIM_ALPHA,
    SelectionState,
};

#[test]
fn ui_palette_uses_stable_semantic_roles() {
    let ui = ReviewTheme::default().ui;
    assert_ne!(ui.surface, ui.surface_selected);
    assert_ne!(ui.text_secondary, ui.text_muted);
    assert_ne!(ui.info, ui.text_muted);
    assert_ne!(ui.warning, ui.accent);
    assert_eq!(ui.scrim.a, SCRIM_ALPHA);
}

#[test]
fn semantic_states_resolve_before_renderer_conversion() {
    let ui = ReviewTheme::default().ui;
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
    assert_eq!(warning.foreground, ui.warning);
    assert!(warning.emphasized);
}
