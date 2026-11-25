mod common;

use clankerdiff_core::{
    DiffReviewCommand as C, DiffReviewEvent, DiffScope, InteractionPhase, RepositoryAction, ReviewCapabilities,
    ReviewCommand as R, ViewMode, testing::DocumentBuilder,
};
use clankerdiff_rouge::{ReviewController, ReviewOutput, RunCommand, diff_view, shortcut_label};
use common::ReviewHarness;
use rouge_core::testing::TestUiBuilder;
use rouge_reactive::Runtime;

#[test]
fn direct_and_typed_commands_share_phase_and_capability_gates() {
    let mut review = ReviewHarness::new(common::document());
    review.handle.set_capabilities(ReviewCapabilities::NONE);
    for command in [C::Refresh, C::StageAll, C::BeginCommit, C::CycleScope, C::SubmitReview] {
        assert!(!review.handle.with(|state| state.command_enabled(&command)));
        assert!(!review.handle.handle_command(command.clone()));
        review.ui.dispatch_action(RunCommand(command));
    }
    assert!(review.outputs().is_empty());
    assert!(review.handle.handle_command(C::ToggleSourceView));
    assert!(!review.handle.handle_command(R::BeginComment));
    assert!(!review.handle.handle_command(C::ToggleLayout));
    assert!(review.handle.handle_command(R::ShowHelp));
    review.ui.advance(0.0);
    review.ui.type_text("ov");
    assert!(review.handle.with(|state| state.session().shows_source()));
    review.press("escape");
    assert!(review.outputs().is_empty());
    assert!(review.handle.handle_command(C::ToggleSourceView));
    assert!(review.handle.handle_command(R::BeginComment));
    review.ui.dispatch_action(RunCommand(C::ToggleLayout));
    assert_eq!(review.handle.status().get().context.phase, InteractionPhase::Draft);
    assert_eq!(review.handle.status().get().view_mode, ViewMode::Auto);
}

#[test]
fn draft_crud_uses_the_session_and_emits_the_existing_submission() {
    let review = ReviewHarness::new(common::document());
    assert!(!review.handle.handle_command(R::EditComment));
    assert!(review.handle.handle_command(R::BeginComment));
    assert!(review.handle.handle_command(R::SubmitComment));
    assert!(review.handle.with(|state| state.session().review().is_empty()));
    assert!(review.handle.handle_command(R::BeginComment));
    review.handle.set_draft_body("original");
    assert!(review.handle.handle_command(R::SubmitComment));
    assert!(review.handle.handle_command(R::EditComment));
    review.handle.set_draft_body("cancelled");
    assert!(review.handle.handle_command(R::Cancel));
    assert!(review.handle.handle_command(R::EditComment));
    review.handle.set_draft_body("edited");
    assert!(review.handle.handle_command(R::SubmitComment));
    assert!(review.handle.handle_command(C::SubmitReview));
    assert!(
        matches!(&*review.outputs(), [ReviewOutput::Event(DiffReviewEvent::SubmitReview(submitted))] if submitted.formatted.contains("edited"))
    );
    assert!(review.handle.handle_command(R::DeleteComment));
    assert!(review.handle.with(|state| state.session().review().is_empty()));
}

#[test]
fn repository_completion_is_independent_of_snapshot_refresh() {
    let review = ReviewHarness::new(common::document());
    assert!(review.handle.handle_command(C::StageAll));
    assert!(!review.handle.handle_command(C::StageAll));
    assert!(!review.handle.handle_command(C::Refresh));
    let projection = review.handle.presentation_revision().get();
    review.handle.set_background_error(Some("background".to_owned()));
    assert!(review.handle.status().get().context.repository_pending);
    review.handle.command_finished(Err("command".to_owned()));
    assert_eq!(review.handle.presentation_revision().get(), projection);
    assert_eq!(review.handle.status().get().repository_error.as_deref(), Some("command"));
    assert!(review.handle.handle_command(C::Refresh));
    assert_eq!(
        &*review.outputs(),
        [
            ReviewOutput::Event(DiffReviewEvent::RepositoryAction(RepositoryAction::StageAll)),
            ReviewOutput::Event(DiffReviewEvent::Refresh)
        ]
    );
}

#[test]
fn compatible_refresh_preserves_draft_and_target_change_clears_it() {
    let review = ReviewHarness::new(common::document());
    assert!(review.handle.handle_command(R::BeginComment));
    review.handle.set_draft_body("in progress");
    review.handle.set_document(DocumentBuilder::new().changed("src/lib.rs", "old\n", "new\n").build());
    assert_eq!(
        review.handle.with(|state| state.session().draft().map(|draft| draft.body().to_owned())).as_deref(),
        Some("in progress")
    );
    review
        .handle
        .set_document(DocumentBuilder::new().target(DiffScope::Staged).changed("src/lib.rs", "old\n", "new\n").build());
    assert_eq!(review.handle.status().get().context.phase, InteractionPhase::Browse);
}

#[test]
fn shortcut_labels_are_specific_to_the_command_value() {
    assert_eq!(shortcut_label(&C::First).as_deref(), Some("g / Home"));
    assert_eq!(shortcut_label(&C::Last).as_deref(), Some("G / End"));
    assert_eq!(shortcut_label(&C::Refresh), None);
}

#[test]
fn disposed_review_rejects_late_callbacks() {
    let _runtime = Runtime::new().enter();
    let (ui, handle) = TestUiBuilder::default().build_with(|| {
        let handle = ReviewController::new(common::document()).mount(|_| panic!("unexpected output"));
        (diff_view(handle), handle)
    });
    assert!(handle.is_alive());
    drop(ui);
    assert!(!handle.is_alive());
    handle.set_document(common::document());
    handle.command_finished(Err("late failure".to_owned()));
    assert!(!handle.handle_command(C::SubmitReview));
}

#[test]
fn command_palette_lists_browse_commands_while_open() {
    let mut review = ReviewHarness::new(common::document());
    assert!(review.handle.handle_command(R::ShowHelp));
    review.ui.advance(0.0);
    review.assert_visible("command-5");
}

#[test]
fn a_commit_without_a_message_is_refused_under_the_message_field() {
    let mut review = ReviewHarness::new(common::document());
    assert!(review.handle.handle_command(C::BeginCommit));
    review.ui.advance(0.0);
    review.assert_hidden("commit-field-note");

    review.press("enter");

    assert_eq!(review.ui.text_of("commit-field-note").as_deref(), Some("Commit message cannot be empty"));
}
