use clankerdiff_core::{
    DiffDocument, DiffSide, LookupAnswer, LookupRequest, SourceLocation, TargetLocation,
    testing::{DocumentBuilder, ResolutionBuilder, external, location, source},
};
use clankerdiff_ratatui::{
    DiffReviewCommand, FocusPane, InputOutcome, KeyCode, KeyModifiers,
    testing::{ReviewHarness, ReviewHarnessBuilder, key, key_with},
};
use clankerdiff_symbols::NamePick;
use ratatui::style::Color;
use std::{error::Error, sync::Arc};

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
    assert!(matches!(harness.state().lookup(), Some(NamePick::Card(_))));
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
fn card_docs_render_markdown_with_rustdoc_links_as_their_labels() -> Result<(), Box<dyn Error>> {
    let document = DocumentBuilder::new()
        .changed(
            "src/lib.rs",
            "",
            "/// Wraps [`Vec`] like [Box], unlike x[i].\n///\n/// - keeps [order][Ordering]\n///\n/// [Ordering]: std::cmp::Ordering\npub fn wrap() {}\n\nfn main() {\n    wrap();\n}\n",
        )
        .build();
    let mut harness = ReviewHarness::new(document, 120, 40);
    select(&mut harness, "src/lib.rs", 9)?;
    harness.press(KeyCode::Char('D'));
    harness.draw();
    harness.assert_contains("Wraps Vec like Box, unlike x[i].");
    harness.assert_contains("• keeps order");
    Ok(())
}

#[test]
fn hinted_names_and_the_card_name_are_highlighted_in_the_source_line() -> Result<(), Box<dyn Error>> {
    let mut harness = harness();
    select(&mut harness, "src/main.rs", 2)?;
    harness.draw();
    let [double, add, total] = name_backgrounds(&harness)?;
    assert!(double == total && add == total);
    harness.press(KeyCode::Char('D'));
    harness.draw();
    let [double, add, total] = name_backgrounds(&harness)?;
    assert!(double != total && add != total);
    harness.press(KeyCode::Char('a'));
    harness.draw();
    let [double, add, total] = name_backgrounds(&harness)?;
    assert!(double != total && add == total);
    Ok(())
}

#[test]
fn the_sticky_file_header_shows_the_symbols_enclosing_the_top_line() -> Result<(), Box<dyn Error>> {
    let steps = "        step();\n".repeat(30);
    let document = DocumentBuilder::new()
        .changed("src/lib.rs", "", &format!("mod outer {{\n    fn long() {{\n{steps}    }}\n}}\n"))
        .build();
    let mut harness = ReviewHarness::new(document, 100, 12);
    harness.draw();
    select(&mut harness, "src/lib.rs", 30)?;
    harness.draw();
    harness.assert_contains("src/ › outer › long");
    Ok(())
}

#[test]
fn enter_while_a_name_is_looked_up_jumps_to_the_language_servers_answer() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarnessBuilder { document: document(), ..ReviewHarnessBuilder::default() }
        .dimensions(120, 40)
        .with_lookups()
        .build();
    let origin = location("src/main.rs", 3)?;
    select(&mut harness, "src/main.rs", 3)?;
    harness.press(KeyCode::Char('D'));
    let lookup = harness.last_lookup().ok_or("lookup")?;
    assert!(
        matches!(&lookup.request, LookupRequest::Definition(request) if request.at == origin && request.column == 4)
    );
    harness.draw();
    harness.assert_contains("src/lib.rs:2 · looking up · 2 uses in this diff");

    harness.press(KeyCode::Enter);
    assert_eq!(selected(&harness), Some(origin));
    harness.answer_lookup(lookup, Ok(ResolutionBuilder::new().target("src/math.rs", 1).answer()));
    assert_eq!(selected(&harness), Some(location("src/math.rs", 1)?));
    Ok(())
}

#[test]
fn an_external_definition_opens_read_only_refreshes_and_escape_returns() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarnessBuilder { document: document(), ..ReviewHarnessBuilder::default() }
        .dimensions(120, 40)
        .with_lookups()
        .build();
    let origin = location("src/main.rs", 3)?;
    select(&mut harness, "src/main.rs", 3)?;
    harness.press(KeyCode::Char('D'));
    let lookup = harness.last_lookup().ok_or("lookup")?;
    harness.answer_lookup(lookup, Ok(ResolutionBuilder::new().external(4, STD, 2).preview("pub struct Arc;").answer()));
    harness.press(KeyCode::Enter);
    let std = harness.last_lookup().ok_or("the source is requested")?;
    assert_eq!(std.request, LookupRequest::Source(external(4, STD)));
    harness.draw();
    harness.assert_contains("Opening std · alloc/src/sync.rs…");

    harness.answer_lookup(std.clone(), Ok(LookupAnswer::Source(source("// std\npub struct Arc;\n"))));
    harness.draw();
    harness.assert_contains("pub struct Arc;");
    harness.assert_contains("Not in this diff");
    let target = TargetLocation { path: external(4, STD), side: DiffSide::New, line_number: 2 };
    assert_eq!(harness.state().session().current_location(), Some(target));
    harness.state_mut().set_document(DocumentBuilder::new().changed("src/main.rs", "", MAIN).build());
    let refresh = harness.state_mut().take_lookups().pop().ok_or("the open reference is refreshed")?;
    assert_eq!(refresh.request, std.request);
    harness.press(KeyCode::Esc);
    assert_eq!(selected(&harness), Some(origin));
    assert!(harness.state().session().reference().is_none());
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
    ReviewHarness::new(document(), 120, 40)
}

fn document() -> Arc<DiffDocument> {
    DocumentBuilder::new()
        .changed("lib/math.rs", "", "fn add() {}\n")
        .changed("src/lib.rs", "", "/// Doubles a number.\npub fn double(value: u32) -> u32 {\n    value * 2\n}\n")
        .changed("src/main.rs", "", MAIN)
        .changed("src/math.rs", "", "pub fn add(left: u32, right: u32) -> u32 {\n    left + right\n}\n")
        .build()
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

fn name_backgrounds(harness: &ReviewHarness) -> Result<[Option<Color>; 3], Box<dyn Error>> {
    let line = "let total = double(add(1, 2));";
    let row = harness.row_containing(line).ok_or("line is not visible")?;
    let text = harness.row_text(row);
    let start = text.find(line).ok_or("line is not visible")?;
    let column = |name: &str| -> Result<Option<Color>, Box<dyn Error>> {
        let offset = start + line.find(name).ok_or("name is not on the line")?;
        let x = u16::try_from(text[..offset].chars().count())?;
        Ok(harness.buffer().cell((x, row)).and_then(|cell| cell.style().bg))
    };
    Ok([column("double")?, column("add")?, column("total")?])
}

const STD: &str = "std · alloc/src/sync.rs";
const MAIN: &str = "fn main() {\n    let total = double(add(1, 2));\n    double(total);\n    missing();\n}\n";
