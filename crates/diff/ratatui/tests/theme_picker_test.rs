use clankerdiff_core::testing::DocumentBuilder;
use clankerdiff_markdown::MarkdownDocument;
use clankerdiff_ratatui::{
    DiffReviewState, DiffReviewWidget, InputOutcome, InteractionPhase, KeyCode, KeyEvent,
    KeyModifiers, MarkdownReviewState, ReviewCommand, ReviewInput, ThemeChoice,
};
use clankerdiff_theme::{ReviewTheme, ThemeId};
use ratatui::{
    buffer::{Buffer, Cell},
    layout::Rect,
    widgets::StatefulWidget,
};
use std::{error::Error, sync::Arc};

#[test]
fn custom_theme_choices_preserve_order_labels_and_commit_explicitly() {
    let original = ReviewTheme::builder("original").build();
    let choices = choices();
    let mut state = DiffReviewState::with_theme(
        DocumentBuilder::new().changed("a", "old", "new").build(),
        original.clone(),
    );
    assert!(state.theme_choices().is_empty());
    assert!(matches!(
        state.handle_command(ReviewCommand::OpenThemePicker),
        InputOutcome::Ignored
    ));
    state.set_theme_choices(choices.as_slice());
    assert_eq!(state.theme_choices()[0].name, "Zebra custom");
    state.handle_command(ReviewCommand::OpenThemePicker);
    let area = Rect::new(0, 0, 80, 20);
    let mut buffer = Buffer::empty(area);
    DiffReviewWidget::new().render(area, &mut buffer, &mut state);
    let text: String = buffer.content.iter().map(Cell::symbol).collect();
    assert!(text.find("Zebra custom") < text.find("Alpha custom"));
    assert!(text.contains("Zebra custom") && text.contains("Alpha custom"));
    assert!(!text.contains("Ayu"));
    assert!(matches!(
        state.handle_command(ReviewCommand::SelectTheme(99)),
        InputOutcome::Ignored
    ));
    assert_eq!(state.theme().id(), original.id());
    state.handle_command(ReviewCommand::SelectTheme(1));
    assert_eq!(state.theme().id(), choices[1].theme.id());
    assert_eq!(state.interaction_phase(), InteractionPhase::ThemePicker);
    state.handle_command(ReviewCommand::Cancel);
    assert_eq!(state.theme().id(), original.id());
    state.handle_command(ReviewCommand::OpenThemePicker);
    assert!(
        matches!(state.handle_command(ReviewCommand::CommitTheme), InputOutcome::ThemeSelected(ThemeId::Custom(id)) if id == "zebra")
    );
    assert_eq!(state.theme().id(), choices[0].theme.id());
    assert_eq!(state.interaction_phase(), InteractionPhase::Browse);
    state.handle_command(ReviewCommand::OpenThemePicker);
    let _ = state.handle_input(ReviewInput::Key(KeyEvent::new(
        KeyCode::Down,
        KeyModifiers::CONTROL,
    )));
    assert!(
        matches!(state.handle_command(ReviewCommand::CommitTheme), InputOutcome::ThemeSelected(ThemeId::Custom(id)) if id == "zebra")
    );
}

#[test]
fn replacing_choices_or_theme_cancels_an_active_preview() {
    let mut state = DiffReviewState::new(DocumentBuilder::new().build());
    let original = state.theme().id().clone();
    state.set_theme_choices(choices());
    state.handle_command(ReviewCommand::OpenThemePicker);
    state.handle_command(ReviewCommand::SelectTheme(1));
    state.set_theme_choices(Vec::new());
    assert_eq!(state.theme().id(), &original);
    assert_eq!(state.interaction_phase(), InteractionPhase::Browse);
    state.set_theme_choices(choices());
    state.handle_command(ReviewCommand::OpenThemePicker);
    let replacement = ReviewTheme::builder("host override").build();
    state.set_theme(replacement.clone());
    assert_eq!(state.interaction_phase(), InteractionPhase::Browse);
    assert_eq!(state.theme().id(), replacement.id());
}

#[test]
fn markdown_custom_themes_preview_cancel_and_emit_commit() -> Result<(), Box<dyn Error>> {
    let choices = choices();
    let mut state = MarkdownReviewState::with_theme(
        Arc::new(MarkdownDocument::parse("# Review")),
        choices[1].theme.clone(),
    );
    assert!(matches!(
        state.handle_command(ReviewCommand::OpenThemePicker)?,
        InputOutcome::Ignored
    ));
    state.set_theme_choices(choices.as_slice());
    state.handle_command(ReviewCommand::OpenThemePicker)?;
    assert!(matches!(
        state.handle_command(ReviewCommand::SelectTheme(100))?,
        InputOutcome::Ignored
    ));
    assert!(
        matches!(state.handle_command(ReviewCommand::CommitTheme)?, InputOutcome::ThemeSelected(ThemeId::Custom(id)) if id == "alpha")
    );
    state.handle_command(ReviewCommand::OpenThemePicker)?;
    state.handle_command(ReviewCommand::MoveTheme(isize::MIN))?;
    assert_eq!(state.theme().id(), choices[0].theme.id());
    assert_eq!(state.interaction_phase(), InteractionPhase::ThemePicker);
    state.handle_command(ReviewCommand::Cancel)?;
    assert_eq!(state.theme().id(), choices[1].theme.id());
    state.handle_command(ReviewCommand::OpenThemePicker)?;
    state.handle_command(ReviewCommand::SelectTheme(0))?;
    state.set_theme_choices(Vec::new());
    assert_eq!(state.theme().id(), choices[1].theme.id());
    assert_eq!(state.interaction_phase(), InteractionPhase::Browse);
    Ok(())
}

fn choices() -> Vec<ThemeChoice> {
    vec![
        ThemeChoice::new("Zebra custom", ReviewTheme::builder("zebra").build()),
        ThemeChoice::new("Alpha custom", ReviewTheme::builder("alpha").build()),
    ]
}
