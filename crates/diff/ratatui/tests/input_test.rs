use clankerdiff_core::{DiffSide, ViewMode, testing::location};
use clankerdiff_ratatui::{
    FocusPane, InteractionPhase, KeyModifiers, MouseEventKind, ReviewInput,
    testing::{ReviewHarness, caller_and_callee_document, mouse},
};
use std::error::Error;

#[test]
fn clicking_a_line_selects_it_without_starting_a_comment() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarness::new(caller_and_callee_document(), 120, 20);
    harness.draw();
    harness.click("double(1);")?;
    assert_eq!(harness.state().session().selected_source_line(), Some(location("src/main.rs", 2)?));
    assert_eq!(harness.state().focus(), FocusPane::Diff);
    assert_eq!(harness.state().interaction_phase(), InteractionPhase::Browse);
    Ok(())
}

#[test]
fn hovering_a_line_offers_a_comment_button_in_its_gutter() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarness::new(caller_and_callee_document(), 120, 20);
    harness.draw();
    let line = harness.position_of("double(1);").ok_or("the call is shown")?;
    assert!(!harness.row_text(line.y).contains(" + "));
    harness.hover("double(1);")?;
    harness.draw();
    let gutter = harness.position_in("   + ", "+").ok_or_else(|| format!("no comment button:\n{}", harness.text()))?;
    assert_eq!(gutter.y, line.y);
    harness.click_at(gutter);
    assert_eq!(harness.state().interaction_phase(), InteractionPhase::Draft);
    assert_eq!(harness.state().session().selected_source_line(), Some(location("src/main.rs", 2)?));
    Ok(())
}

#[test]
fn clicking_a_side_of_a_split_row_selects_that_side() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarness::new(caller_and_callee_document(), 160, 20);
    harness.state_mut().set_view_mode(ViewMode::Split);
    harness.draw();
    harness.click("fn main() {}")?;
    assert_eq!(harness.state().selected_side(), DiffSide::Old);
    let new = harness.position_in("fn main() {\u{20}", "n main").ok_or("the new line is shown")?;
    harness.click_at(new);
    assert_eq!(harness.state().selected_side(), DiffSide::New);
    Ok(())
}

#[test]
fn clicking_a_directory_folds_and_unfolds_it() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarness::new(caller_and_callee_document(), 120, 20);
    harness.draw();
    let directory = harness.position_in("▾ src", "src").ok_or("the directory is shown")?;
    harness.click_at(directory);
    harness.draw();
    assert!(harness.position_of("▸ src").is_some() && harness.row_containing("    lib.rs").is_none());
    harness.click_at(directory);
    harness.draw();
    assert!(harness.row_containing("    lib.rs").is_some());
    Ok(())
}

#[test]
fn holding_ctrl_without_moving_reveals_links_and_opens_the_card_under_the_pointer() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarness::new(caller_and_callee_document(), 120, 20);
    harness.draw();
    let name = harness.position_of("double(1);").ok_or("the call is shown")?;
    harness.input(mouse(MouseEventKind::Moved, name.x + 1, name.y));
    assert!(harness.state().definition_card().is_none());
    harness.input(ReviewInput::Modifiers(KeyModifiers::CONTROL));
    assert_eq!(harness.state().definition_card().map(|card| card.name.as_str()), Some("double"));
    harness.input(ReviewInput::Modifiers(KeyModifiers::NONE));
    harness.input(mouse(MouseEventKind::Moved, 0, 0));
    assert!(harness.state().definition_card().is_none());
    Ok(())
}
