#![cfg(feature = "test-support")]

use clankerdiff_core::{DiffDocument, DiffScope, testing::DocumentBuilder};
use clankerdiff_ratatui::{
    DiffReviewCommand, DiffReviewEvent, InputOutcome, InteractionPhase, KeyCode, ReviewCommand,
    ReviewTheme, ThemeChoice,
    testing::{ReviewHarness, key},
};
use std::sync::Arc;

#[test]
fn closing_a_draft_installs_only_the_newest_document() {
    for close in [ReviewCommand::Cancel, ReviewCommand::SubmitComment] {
        let mut harness = review();
        harness
            .state_mut()
            .handle_command(ReviewCommand::BeginComment);
        harness.type_text("review note");
        assert_eq!(harness.state().interaction_phase(), InteractionPhase::Draft);
        let installed = harness.state().document().clone();
        harness.state_mut().set_document(document(2));
        harness.state_mut().set_document(document(3));
        harness.state_mut().set_scope(DiffScope::Staged);
        assert!(Arc::ptr_eq(harness.state().document(), &installed));
        assert_eq!(harness.state().scope(), DiffScope::Both);
        harness.state_mut().handle_command(close);
        assert_eq!(harness.state().document(), &document(3));
        assert_eq!(harness.state().scope(), DiffScope::Staged);
        if close == ReviewCommand::SubmitComment {
            assert_eq!(harness.state().review().comments().len(), 1);
        }
    }
}

#[test]
fn confirming_a_prompt_emits_its_action_and_installs_the_held_document() {
    let mut harness = review();
    harness
        .state_mut()
        .handle_command(DiffReviewCommand::BeginCommit);
    harness.type_text("commit message");
    assert_eq!(
        harness.state().interaction_phase(),
        InteractionPhase::RepositoryPrompt
    );
    harness.state_mut().set_document(document(2));
    assert_ne!(harness.state().document(), &document(2));
    let outcome = harness.state_mut().handle_input(key(KeyCode::Enter));
    assert!(matches!(
        outcome,
        InputOutcome::Emitted(DiffReviewEvent::RepositoryAction(_))
    ));
    assert_eq!(harness.state().document(), &document(2));
}

#[test]
fn help_and_theme_picker_hold_updates_until_closed() {
    for command in [ReviewCommand::ShowHelp, ReviewCommand::OpenThemePicker] {
        let mut harness = review();
        harness
            .state_mut()
            .set_theme_choices(vec![ThemeChoice::new("test", ReviewTheme::default())]);
        harness.state_mut().handle_command(command);
        let phase = harness.state().interaction_phase();
        assert_ne!(phase, InteractionPhase::Browse);
        harness.state_mut().set_document(document(2));
        assert_eq!(harness.state().interaction_phase(), phase);
        assert_ne!(harness.state().document(), &document(2));
        harness.state_mut().handle_command(ReviewCommand::Cancel);
        assert_eq!(harness.state().document(), &document(2));
    }
}

#[test]
fn health_updates_apply_during_a_draft() {
    let mut harness = review();
    harness
        .state_mut()
        .handle_command(ReviewCommand::BeginComment);
    harness
        .state_mut()
        .set_background_error(Some("index".into()));
    assert_eq!(harness.state().repository_error(), Some("index"));
    assert_eq!(harness.state().interaction_phase(), InteractionPhase::Draft);
}

#[test]
fn loading_discards_a_held_document() {
    let mut harness = review();
    harness
        .state_mut()
        .handle_command(ReviewCommand::BeginComment);
    harness.state_mut().set_document(document(2));
    harness.state_mut().set_loading();
    assert_eq!(
        harness.state().interaction_phase(),
        InteractionPhase::Browse
    );
    harness.state_mut().handle_command(ReviewCommand::Cancel);
    assert_ne!(harness.state().document(), &document(2));
}

#[test]
fn replacement_reconciles_comments_and_selection_without_changing_theme() {
    let mut harness = review();
    harness
        .state_mut()
        .handle_command(ReviewCommand::BeginComment);
    harness.type_text("retain this comment");
    harness
        .state_mut()
        .handle_command(ReviewCommand::SubmitComment);
    let comments = harness.state().review().comments().to_vec();
    let theme = harness.state().theme().clone();
    harness.state_mut().set_document(
        DocumentBuilder::new()
            .changed("before.rs", "old\n", "inserted\n")
            .changed("file.rs", "old\n", "1\n")
            .build(),
    );
    let selected = harness.state().selected_file().expect("a selected file");
    assert_eq!(
        harness.state().document().files[selected].path.as_str(),
        "file.rs"
    );
    assert_eq!(harness.state().review().comments(), comments);
    assert_eq!(harness.state().theme().id(), theme.id());
}

fn review() -> ReviewHarness {
    ReviewHarness::new(document(1), 80, 24)
}

fn document(version: u64) -> Arc<DiffDocument> {
    DocumentBuilder::new()
        .changed("file.rs", "old\n", &format!("{version}\n"))
        .build()
}
