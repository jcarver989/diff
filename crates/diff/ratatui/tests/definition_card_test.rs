use clankerdiff_core::{
    SourceLocation,
    testing::{DocumentBuilder, location},
};
use clankerdiff_ratatui::{
    DiffReviewCommand, FocusPane, InputOutcome, KeyCode, KeyModifiers,
    testing::{ReviewHarness, key, key_with},
};
use clankerdiff_symbols::Lookup;
use std::error::Error;

#[test]
fn a_line_with_one_name_opens_its_card_and_enter_jumps_with_history() -> Result<(), Box<dyn Error>> {
    let mut harness = harness();
    select(&mut harness, "src/main.rs", 3)?;
    harness.press(KeyCode::Char('D'));
    harness.draw();
    harness.assert_contains("pub fn double(value: u32) -> u32");
    harness.assert_contains("Doubles a number.");
    harness.assert_contains("src/lib.rs:2 · 2 uses in this diff");
    harness.press(KeyCode::Enter);
    assert!(harness.state().lookup().is_none());
    assert_eq!(selected(&harness), Some(location("src/lib.rs", 2)?));
    harness.input(key_with(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert_eq!(selected(&harness), Some(location("src/main.rs", 3)?));
    harness.input(key_with(KeyCode::Char('p'), KeyModifiers::CONTROL));
    assert_eq!(selected(&harness), Some(location("src/lib.rs", 2)?));
    Ok(())
}

#[test]
fn several_names_offer_hint_keys_and_tab_cycles_repeated_definitions() -> Result<(), Box<dyn Error>> {
    let mut harness = harness();
    select(&mut harness, "src/main.rs", 2)?;
    harness.press(KeyCode::Char('D'));
    harness.draw();
    harness.assert_contains("show definition: [a] double  [s] add  [Esc] cancel");
    harness.press(KeyCode::Char('s'));
    harness.draw();
    harness.assert_contains("pub fn add(left: u32, right: u32) -> u32");
    harness.assert_contains("src/math.rs:1 · 1 of 2");
    harness.press(KeyCode::Tab);
    harness.draw();
    harness.assert_contains("lib/math.rs:1 · 2 of 2");
    harness.press(KeyCode::Enter);
    assert_eq!(selected(&harness), Some(location("lib/math.rs", 1)?));
    Ok(())
}

#[test]
fn escape_closes_hints_and_cards_without_leaving_the_review() -> Result<(), Box<dyn Error>> {
    let mut harness = harness();
    select(&mut harness, "src/main.rs", 2)?;
    for keys in [&[KeyCode::Char('D')][..], &[KeyCode::Char('D'), KeyCode::Char('a')]] {
        for code in keys {
            harness.press(*code);
        }
        assert!(harness.state().lookup().is_some());
        assert_eq!(harness.state_mut().handle_input(key(KeyCode::Esc)), InputOutcome::Consumed);
        assert!(harness.state().lookup().is_none());
    }
    Ok(())
}

#[test]
fn other_commands_close_the_card_and_still_run() -> Result<(), Box<dyn Error>> {
    let mut harness = harness();
    select(&mut harness, "src/main.rs", 3)?;
    harness.press(KeyCode::Char('D'));
    assert!(matches!(harness.state().lookup(), Some(Lookup::Card(_))));
    harness.press(KeyCode::Char('j'));
    assert!(harness.state().lookup().is_none());
    assert_eq!(selected(&harness), Some(location("src/main.rs", 4)?));
    Ok(())
}

#[test]
fn a_name_defined_outside_the_diff_says_so() -> Result<(), Box<dyn Error>> {
    let mut harness = harness();
    select(&mut harness, "src/main.rs", 4)?;
    harness.press(KeyCode::Char('D'));
    harness.draw();
    harness.assert_contains("No definition of missing in this diff");
    Ok(())
}

#[test]
fn a_line_without_names_opens_nothing() -> Result<(), Box<dyn Error>> {
    let mut harness = harness();
    select(&mut harness, "src/main.rs", 1)?;
    assert_eq!(harness.state_mut().handle_command(DiffReviewCommand::ShowDefinition), InputOutcome::Ignored);
    assert!(harness.state().lookup().is_none());
    Ok(())
}

fn harness() -> ReviewHarness {
    let document = DocumentBuilder::new()
        .changed("lib/math.rs", "", "fn add() {}\n")
        .changed("src/lib.rs", "", "/// Doubles a number.\npub fn double(value: u32) -> u32 {\n    value * 2\n}\n")
        .changed(
            "src/main.rs",
            "",
            "fn main() {\n    let total = double(add(1, 2));\n    double(total);\n    missing();\n}\n",
        )
        .changed("src/math.rs", "", "pub fn add(left: u32, right: u32) -> u32 {\n    left + right\n}\n")
        .build();
    ReviewHarness::new(document, 120, 40)
}

fn select(harness: &mut ReviewHarness, path: &str, line_number: usize) -> Result<(), Box<dyn Error>> {
    let target = location(path, line_number)?;
    let state = harness.state_mut();
    state.handle_command(DiffReviewCommand::Focus(FocusPane::Diff));
    let row = state.presentation().row_showing_source(&target).ok_or("line is not visible")?;
    state.handle_command(DiffReviewCommand::SelectRow(row));
    Ok(())
}

fn selected(harness: &ReviewHarness) -> Option<SourceLocation> {
    harness.state().session().selected_source_line()
}
