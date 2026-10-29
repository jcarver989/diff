use clankerdiff_ratatui::{
    DiffReviewCommand, InteractionPhase, KeyCode, ReviewCommand,
    testing::{ReviewHarness, caller_and_callee_document},
};
use ratatui::layout::Position;
use std::error::Error;

#[test]
fn question_mark_lists_the_commands_under_their_sections_with_their_keys() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarness::new(caller_and_callee_document(), 120, 40);
    harness.press(KeyCode::Char('?'));
    harness.draw();
    assert_eq!(harness.state().interaction_phase(), InteractionPhase::Help);
    harness.assert_contains("Commands");
    let navigation = harness.row_containing(" Navigation ").ok_or("a section header")?;
    let next = harness.row_containing("Next item").ok_or("the first command")?;
    assert!(navigation < next);
    assert!(harness.row_text(next).contains("j / ↓"), "{}", harness.row_text(next));
    harness.press(KeyCode::Char('?'));
    assert_eq!(harness.state().interaction_phase(), InteractionPhase::Browse);
    Ok(())
}

#[test]
fn typing_filters_the_commands_and_enter_runs_the_best_match() {
    let mut harness = ReviewHarness::new(caller_and_callee_document(), 120, 40);
    let split = harness.state().layout().is_split();
    harness.press(KeyCode::Char('?'));
    harness.type_text("toggle split");
    harness.draw();
    harness.assert_contains("Toggle split / unified");
    assert!(!harness.text().contains("Next item"));
    harness.press(KeyCode::Enter);
    assert_eq!(harness.state().interaction_phase(), InteractionPhase::Browse);
    assert_ne!(harness.state().layout().is_split(), split);
}

#[test]
fn clicking_a_command_runs_it_and_clicking_outside_closes_the_list() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarness::new(caller_and_callee_document(), 120, 40);
    harness.state_mut().handle_command(ReviewCommand::ShowHelp);
    harness.type_text("collapse all");
    harness.draw();
    harness.click("Collapse / expand all files")?;
    assert!(harness.state().session().all_files_collapsed());
    assert_eq!(harness.state().interaction_phase(), InteractionPhase::Browse);

    harness.state_mut().handle_command(ReviewCommand::ShowHelp);
    harness.draw();
    harness.click_at(Position::new(1, 38));
    assert_eq!(harness.state().interaction_phase(), InteractionPhase::Browse);
    Ok(())
}

#[test]
fn commands_with_nothing_to_act_on_are_left_out() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarness::new(caller_and_callee_document(), 120, 40);
    harness.state_mut().handle_command(ReviewCommand::ShowHelp);
    harness.type_text("copy review");
    harness.draw();
    assert!(!harness.text().contains("Copy review"), "{}", harness.text());
    harness.press(KeyCode::Esc);
    let anchor = harness.state().session().selected_anchor().ok_or("a commentable line")?;
    harness.state_mut().review_mut().add_comment(anchor, "note");
    harness.state_mut().handle_command(DiffReviewCommand::Review(ReviewCommand::ShowHelp));
    harness.type_text("copy review");
    harness.draw();
    harness.assert_contains("Copy review");
    Ok(())
}
