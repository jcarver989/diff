use clankerdiff_core::{
    DiffReviewEvent, DiffScope, ReviewTarget,
    testing::{DocumentBuilder, commit_id, location},
};
use clankerdiff_ratatui::{
    DiffReviewCommand, FocusPane, InputOutcome, InteractionPhase,
    testing::{ReviewHarness, caller_and_callee_document},
};
use std::error::Error;

#[test]
fn the_sidebar_toggle_hides_and_restores_the_changed_files() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarness::new(caller_and_callee_document(), 120, 20);
    harness.draw();
    assert!(harness.text().contains("main.rs") && harness.row_containing("    lib.rs").is_some());
    harness.click("◂")?;
    harness.draw();
    assert!(harness.row_containing("    lib.rs").is_none(), "{}", harness.text());
    assert_eq!(harness.state().focus(), FocusPane::Diff);
    harness.click("▸")?;
    harness.draw();
    assert!(harness.row_containing("    lib.rs").is_some());
    Ok(())
}

#[test]
fn collapse_all_folds_every_file_and_then_offers_to_expand_them() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarness::new(caller_and_callee_document(), 120, 20);
    harness.draw();
    harness.click("⊟")?;
    assert!(harness.state().session().all_files_collapsed());
    harness.draw();
    harness.click("⊞")?;
    assert!(!harness.state().session().all_files_collapsed());
    Ok(())
}

#[test]
fn scope_segments_ask_the_host_for_that_scope() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarness::new(caller_and_callee_document(), 120, 20);
    harness.draw();
    let outcome = harness.click("Staged")?;
    assert_eq!(outcome, InputOutcome::Emitted(DiffReviewEvent::SetTarget(DiffScope::Staged.into())));
    Ok(())
}

#[test]
fn the_commit_stepper_steps_between_pages_and_opens_the_picker() -> Result<(), Box<dyn Error>> {
    let document = DocumentBuilder::new()
        .commit("Add parser")
        .commit("Wire the parser")
        .target(commit_id(2))
        .changed("a.rs", "old", "new")
        .build();
    let mut harness = ReviewHarness::new(document, 140, 20);
    harness.draw();
    let previous = harness.click("‹")?;
    assert_eq!(previous, InputOutcome::Emitted(DiffReviewEvent::SetTarget(ReviewTarget::Commit(commit_id(1)))));
    let next = harness.click("›")?;
    assert_eq!(next, InputOutcome::Emitted(DiffReviewEvent::SetTarget(DiffScope::Both.into())));
    harness.click("Wire the parser ▾")?;
    assert_eq!(harness.state().interaction_phase(), InteractionPhase::CommitPicker);
    Ok(())
}

#[test]
fn history_buttons_walk_back_and_forward_once_there_is_history() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarness::new(caller_and_callee_document(), 120, 20);
    harness.draw();
    assert_eq!(harness.click("←")?, InputOutcome::Ignored, "nothing to go back to");
    let origin = harness.state().session().selected_source_line();
    harness.state_mut().handle_command(DiffReviewCommand::JumpTo(location("src/lib.rs", 2)?.into()));
    harness.draw();
    harness.click("←")?;
    assert_eq!(harness.state().session().selected_source_line(), origin);
    harness.draw();
    harness.click("→")?;
    assert_eq!(harness.state().session().selected_source_line(), Some(location("src/lib.rs", 2)?));
    Ok(())
}

#[test]
fn the_review_bar_offers_copy_and_submit_once_there_are_comments() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarness::new(caller_and_callee_document(), 120, 20);
    harness.draw();
    let bar = harness.row_text(18);
    assert!(bar.trim_end().ends_with("Cancel  │") && !bar.contains("Submit"), "{bar}");
    let anchor = harness.state().session().selected_anchor().ok_or("a commentable line")?;
    harness.state_mut().review_mut().add_comment(anchor, "note");
    harness.draw();
    let bar = harness.row_text(18);
    assert!(bar.contains(" Copy ") && bar.contains(" Submit ") && bar.contains("1 review comment"), "{bar}");
    assert!(matches!(harness.click("Submit")?, InputOutcome::Emitted(DiffReviewEvent::SubmitReview(_))));
    assert_eq!(harness.click("Cancel")?, InputOutcome::Emitted(DiffReviewEvent::Cancel));
    Ok(())
}

#[test]
fn hovering_a_control_shows_what_it_does_in_the_review_bar() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarness::new(caller_and_callee_document(), 120, 20);
    harness.draw();
    harness.hover("◂")?;
    harness.draw();
    assert!(harness.row_text(18).contains("Hide changed files (b)"), "{}", harness.row_text(18));
    Ok(())
}

#[test]
fn a_narrow_toolbar_drops_the_change_totals_before_the_layout_controls() {
    let mut harness = ReviewHarness::new(caller_and_callee_document(), 120, 20);
    harness.draw();
    assert!(harness.row_text(1).contains("+6 −1"));
    let mut narrow = ReviewHarness::new(caller_and_callee_document(), 70, 20);
    narrow.draw();
    let toolbar = narrow.row_text(1);
    assert!(!toolbar.contains("+6 −1") && toolbar.contains("Split"), "{toolbar}");
}
