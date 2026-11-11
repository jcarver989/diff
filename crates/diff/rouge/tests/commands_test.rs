mod common;

use clankerdiff_core::{
    DiffReviewCommand as C, DiffReviewEvent, DiffScope, InteractionPhase, RepositoryAction, ReviewCapabilities,
    ReviewCommand as R, ViewMode, testing::DocumentBuilder,
};
use clankerdiff_rouge::{ReviewOutput, RunCommand, shortcut_label};
use rouge_core::Key;

#[test]
fn direct_and_typed_commands_share_phase_and_capability_gates() {
    common::run(common::document(), |ui, handle, outputs| {
        handle.set_capabilities(ReviewCapabilities::NONE);
        for command in [C::Refresh, C::StageAll, C::BeginCommit, C::CycleScope, C::SubmitReview] {
            assert!(!handle.with(|state| state.command_enabled(&command)));
            assert!(!handle.handle_command(command.clone()));
            ui.dispatch_action(RunCommand(command));
        }
        assert!(outputs.borrow().is_empty());
        assert!(handle.handle_command(C::ToggleSourceView));
        assert!(!handle.handle_command(R::BeginComment));
        assert!(!handle.handle_command(C::ToggleLayout));
        assert!(handle.handle_command(R::ShowHelp));
        ui.advance(0.0);
        ui.type_text("ov");
        assert!(handle.with(|state| state.session().shows_source()));
        ui.press(Key::Escape);
        assert!(outputs.borrow().is_empty());
        assert!(handle.handle_command(C::ToggleSourceView));
        assert!(handle.handle_command(R::BeginComment));
        ui.dispatch_action(RunCommand(C::ToggleLayout));
        assert_eq!(handle.status().get().context.phase, InteractionPhase::Draft);
        assert_eq!(handle.status().get().view_mode, ViewMode::Auto);
    });
}

#[test]
fn draft_crud_uses_the_session_and_emits_the_existing_submission() {
    common::run(common::document(), |_, handle, outputs| {
        assert!(!handle.handle_command(R::EditComment));
        assert!(handle.handle_command(R::BeginComment));
        assert!(handle.handle_command(R::SubmitComment));
        assert!(handle.with(|state| state.session().review().is_empty()));
        assert!(handle.handle_command(R::BeginComment));
        handle.set_draft_body("original");
        assert!(handle.handle_command(R::SubmitComment));
        assert!(handle.handle_command(R::EditComment));
        handle.set_draft_body("cancelled");
        assert!(handle.handle_command(R::Cancel));
        assert!(handle.handle_command(R::EditComment));
        handle.set_draft_body("edited");
        assert!(handle.handle_command(R::SubmitComment));
        assert!(handle.handle_command(C::SubmitReview));
        assert!(
            matches!(outputs.borrow().as_slice(), [ReviewOutput::Event(DiffReviewEvent::SubmitReview(review))] if review.formatted.contains("edited"))
        );
        assert!(handle.handle_command(R::DeleteComment));
        assert!(handle.with(|state| state.session().review().is_empty()));
    });
}

#[test]
fn repository_completion_is_independent_of_snapshot_refresh() {
    common::run(common::document(), |_, handle, outputs| {
        assert!(handle.handle_command(C::StageAll));
        assert!(!handle.handle_command(C::StageAll));
        assert!(!handle.handle_command(C::Refresh));
        let projection = handle.presentation_revision().get();
        handle.set_background_error(Some("background".to_owned()));
        assert!(handle.status().get().context.repository_pending);
        handle.command_finished(Err("command".to_owned()));
        assert_eq!(handle.presentation_revision().get(), projection);
        assert_eq!(handle.status().get().repository_error.as_deref(), Some("command"));
        assert!(handle.handle_command(C::Refresh));
        assert_eq!(
            outputs.borrow().as_slice(),
            [
                ReviewOutput::Event(DiffReviewEvent::RepositoryAction(RepositoryAction::StageAll)),
                ReviewOutput::Event(DiffReviewEvent::Refresh)
            ]
        );
    });
}

#[test]
fn compatible_refresh_preserves_draft_and_target_change_clears_it() {
    common::run(common::document(), |_, handle, _| {
        assert!(handle.handle_command(R::BeginComment));
        handle.set_draft_body("in progress");
        handle.set_document(DocumentBuilder::new().changed("src/lib.rs", "old\n", "new\n").build());
        assert_eq!(
            handle.with(|state| state.session().draft().map(|draft| draft.body().to_owned())).as_deref(),
            Some("in progress")
        );
        handle.set_document(
            DocumentBuilder::new().target(DiffScope::Staged).changed("src/lib.rs", "old\n", "new\n").build(),
        );
        assert_eq!(handle.status().get().context.phase, InteractionPhase::Browse);
    });
}

#[test]
fn shortcut_labels_are_specific_to_the_command_value() {
    assert_eq!(shortcut_label(&C::First).as_deref(), Some("g / Home"));
    assert_eq!(shortcut_label(&C::Last).as_deref(), Some("G / End"));
    assert_eq!(shortcut_label(&C::Refresh), None);
}

#[test]
fn disposed_review_rejects_late_callbacks() {
    use clankerdiff_rouge::{ReviewController, diff_view};
    use rouge_core::testing::TestUiBuilder;
    use rouge_reactive::Runtime;

    let _runtime = Runtime::new().enter();
    let (ui, handle) = TestUiBuilder::default().build_with(|| {
        let handle = ReviewController::new(common::document()).mount(|_| panic!("unexpected output"));
        (diff_view(handle.clone()), handle)
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
    common::run(common::document(), |ui, handle, _| {
        assert!(handle.handle_command(R::ShowHelp));
        ui.advance(0.0);
        assert!(ui.center_of("command-5").is_ok());
    });
}
