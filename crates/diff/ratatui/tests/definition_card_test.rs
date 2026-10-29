use clankerdiff_core::{
    DiffDocument, DiffSide, LookupAnswer, LookupRequest, SourceLocation, TargetLocation, ViewMode,
    testing::{DocumentBuilder, ResolutionBuilder, external, location, source},
};
use clankerdiff_ratatui::{
    DiffReviewCommand, FocusPane, InputOutcome, KeyCode, KeyModifiers, MouseButton, MouseEventKind,
    testing::{ReviewHarness, ReviewHarnessBuilder, key, key_with, mouse, mouse_with},
};
use ratatui::style::{Color, Modifier, Style};
use std::{error::Error, sync::Arc};

#[test]
fn a_line_with_one_name_opens_its_card_and_enter_jumps_with_history() -> Result<(), Box<dyn Error>> {
    let mut harness = harness();
    select(&mut harness, "src/main.rs", 3)?;
    harness.press(KeyCode::Char('D'));
    harness.draw();
    harness.assert_contains("pub fn double(value: u32) -> u32");
    harness.assert_contains("Doubles a number.");
    assert_meta(&harness, "src/lib.rs:2", "2 uses in this diff")?;
    harness.press(KeyCode::Enter);
    assert!(harness.state().definition_card().is_none());
    assert_eq!(selected(&harness), Some(location("src/lib.rs", 2)?));
    harness.input(key_with(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert_eq!(selected(&harness), Some(location("src/main.rs", 3)?));
    harness.state_mut().handle_command(DiffReviewCommand::NavigateForward);
    assert_eq!(selected(&harness), Some(location("src/lib.rs", 2)?));
    Ok(())
}

#[test]
fn pressing_d_again_steps_to_the_next_name_and_tab_cycles_repeated_definitions() -> Result<(), Box<dyn Error>> {
    let mut harness = harness();
    select(&mut harness, "src/main.rs", 2)?;
    harness.press(KeyCode::Char('D'));
    assert_eq!(card_name(&harness).as_deref(), Some("double"));
    harness.press(KeyCode::Char('D'));
    harness.draw();
    assert_eq!(card_name(&harness).as_deref(), Some("add"));
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
fn escape_closes_the_card_without_leaving_the_review() -> Result<(), Box<dyn Error>> {
    let mut harness = harness();
    select(&mut harness, "src/main.rs", 2)?;
    for _ in 0..2 {
        harness.press(KeyCode::Char('D'));
        assert!(harness.state().definition_card().is_some());
        assert_eq!(harness.state_mut().handle_input(key(KeyCode::Esc)), InputOutcome::Consumed);
        assert!(harness.state().definition_card().is_none());
    }
    Ok(())
}

#[test]
fn other_commands_close_the_card_and_still_run() -> Result<(), Box<dyn Error>> {
    let mut harness = harness();
    select(&mut harness, "src/main.rs", 3)?;
    harness.press(KeyCode::Char('D'));
    assert!(harness.state().definition_card().is_some());
    harness.press(KeyCode::Char('j'));
    assert!(harness.state().definition_card().is_none());
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
fn only_the_card_name_is_highlighted_in_the_source_line() -> Result<(), Box<dyn Error>> {
    let mut harness = harness();
    select(&mut harness, "src/main.rs", 2)?;
    harness.draw();
    let [double, add, total] = name_backgrounds(&harness)?;
    assert!(double == total && add == total);
    harness.press(KeyCode::Char('D'));
    harness.draw();
    let [double, add, total] = name_backgrounds(&harness)?;
    assert!(double != total && add == total);
    harness.press(KeyCode::Char('D'));
    harness.draw();
    let [double, add, total] = name_backgrounds(&harness)?;
    assert!(double == total && add != total);
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
    assert_meta(&harness, "src/lib.rs:2", "looking up · 2 uses in this diff")?;

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
    assert!(harness.state().definition_card().is_none());
    Ok(())
}

#[test]
fn ctrl_hovering_a_name_underlines_links_and_opens_its_card() -> Result<(), Box<dyn Error>> {
    let mut harness = harness();
    harness.draw();
    let (x, y) = name_at(&harness, TOTAL, "double")?;
    harness.input_and_draw(mouse(MouseEventKind::Moved, x, y));
    harness.input(mouse(MouseEventKind::Moved, x + 1, y));
    assert!(!harness.state().is_dirty());
    assert!(!underlined(&harness, TOTAL, "add")?);
    harness.input(mouse_with(MouseEventKind::Moved, x + 2, y, KeyModifiers::CONTROL));
    harness.draw();
    assert_eq!(card_name(&harness).as_deref(), Some("double"));
    assert_meta(&harness, "src/lib.rs:2", "2 uses in this diff")?;
    assert!(underlined(&harness, TOTAL, "add")?);
    assert!(!underlined(&harness, TOTAL, "let")?);
    harness.input(mouse_with(MouseEventKind::Moved, x + 3, y, KeyModifiers::CONTROL));
    assert!(!harness.state().is_dirty());

    let (_, other) = name_at(&harness, "fn main() {", "main")?;
    harness.input(mouse(MouseEventKind::Moved, x, other));
    harness.draw();
    assert!(harness.state().definition_card().is_none());
    assert!(!underlined(&harness, TOTAL, "add")?);

    harness.input(mouse_with(MouseEventKind::Moved, x, other, KeyModifiers::CONTROL));
    harness.press(KeyCode::Char('j'));
    harness.draw();
    assert!(!underlined(&harness, TOTAL, "add")?);
    Ok(())
}

#[test]
fn hover_cards_follow_the_pointer_but_keyboard_cards_stay_open() -> Result<(), Box<dyn Error>> {
    let mut harness = harness();
    harness.draw();
    let (x, y) = name_at(&harness, TOTAL, "double")?;
    harness.input(mouse_with(MouseEventKind::Moved, x, y, KeyModifiers::ALT));
    harness.draw();
    let (end, _) = name_at(&harness, TOTAL, "2));")?;
    harness.input(mouse(MouseEventKind::Moved, end, y));
    assert_eq!(card_name(&harness).as_deref(), Some("double"));
    let (card_x, card_y) = name_at(&harness, "2 uses in this diff", "uses")?;
    harness.input(mouse(MouseEventKind::Moved, card_x, card_y));
    assert_eq!(card_name(&harness).as_deref(), Some("double"));
    harness.input(mouse(MouseEventKind::Moved, card_x, 0));
    assert!(harness.state().definition_card().is_none());

    select(&mut harness, "src/main.rs", 3)?;
    harness.press(KeyCode::Char('D'));
    harness.draw();
    harness.input(mouse(MouseEventKind::Moved, 0, 0));
    assert_eq!(card_name(&harness).as_deref(), Some("double"));
    Ok(())
}

#[test]
fn ctrl_click_on_a_name_jumps_to_its_definition() -> Result<(), Box<dyn Error>> {
    let mut harness = harness();
    harness.draw();
    let (x, y) = name_at(&harness, TOTAL, "double")?;
    harness.input(mouse_with(MouseEventKind::Down(MouseButton::Left), x, y, KeyModifiers::CONTROL));
    assert_eq!(selected(&harness), Some(location("src/lib.rs", 2)?));
    assert!(harness.state().definition_card().is_none());
    assert_eq!(harness.state().focus(), FocusPane::Diff);
    Ok(())
}

#[test]
fn ctrl_click_away_from_names_selects_the_line_as_usual() -> Result<(), Box<dyn Error>> {
    let mut harness = harness();
    harness.draw();
    let (x, y) = name_at(&harness, TOTAL, "let")?;
    harness.input(mouse_with(MouseEventKind::Down(MouseButton::Left), x, y, KeyModifiers::CONTROL));
    assert!(harness.state().definition_card().is_none());
    assert_eq!(selected(&harness), Some(location("src/main.rs", 2)?));
    Ok(())
}

#[test]
fn alt_click_while_a_name_is_looked_up_jumps_to_the_language_servers_answer() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarnessBuilder { document: document(), ..ReviewHarnessBuilder::default() }
        .dimensions(120, 40)
        .with_lookups()
        .build();
    harness.draw();
    let (x, y) = name_at(&harness, TOTAL, "double")?;
    harness.input(mouse_with(MouseEventKind::Down(MouseButton::Left), x, y, KeyModifiers::ALT));
    let lookup = harness.last_lookup().ok_or("lookup")?;
    assert_ne!(selected(&harness), Some(location("src/math.rs", 1)?));
    harness.answer_lookup(lookup, Ok(ResolutionBuilder::new().target("src/math.rs", 1).answer()));
    assert_eq!(selected(&harness), Some(location("src/math.rs", 1)?));
    Ok(())
}

#[test]
fn ctrl_hover_finds_names_on_the_old_side_of_a_split_row() -> Result<(), Box<dyn Error>> {
    let document = DocumentBuilder::new()
        .changed("src/main.rs", "fn main() {\n    first(1);\n}\n", "fn main() {\n    second(1);\n}\n")
        .build();
    let mut harness = ReviewHarness::new(document, 160, 20);
    harness.draw();
    assert_eq!(harness.row_containing("first(1);"), harness.row_containing("second(1);"));
    for name in ["first", "second"] {
        let (x, y) = name_at(&harness, &format!("{name}(1);"), name)?;
        harness.input(mouse_with(MouseEventKind::Moved, x, y, KeyModifiers::CONTROL));
        assert_eq!(card_name(&harness).as_deref(), Some(name));
    }
    Ok(())
}

#[test]
fn ctrl_hovering_an_unchanged_name_on_the_old_side_shows_the_current_versions_docs() -> Result<(), Box<dyn Error>> {
    let document = DocumentBuilder::new()
        .changed(
            "src/main.rs",
            "fn main() {\n    let one = 1;\n    Arc::new(one);\n}\n",
            "fn main() {\n    let one = 2;\n    Arc::new(one);\n}\n",
        )
        .build();
    let mut harness =
        ReviewHarnessBuilder { document, ..ReviewHarnessBuilder::default() }.dimensions(200, 30).with_lookups().build();
    harness.state_mut().set_view_mode(ViewMode::Split);
    harness.draw();
    let (x, y) = name_at(&harness, "Arc::new(one);", "Arc")?;
    harness.input(mouse_with(MouseEventKind::Moved, x, y, KeyModifiers::CONTROL));
    let card = harness.state().definition_card().ok_or("the old side opens a card")?;
    assert_eq!((card.name.as_str(), card.origin.side), ("Arc", DiffSide::Old));
    let lookup = harness.last_lookup().ok_or("the old side is looked up")?;
    assert!(matches!(&lookup.request, LookupRequest::Definition(request) if request.at == location("src/main.rs", 3)?));

    harness
        .answer_lookup(lookup, Ok(ResolutionBuilder::new().hover("A thread-safe reference-counted pointer.").answer()));
    harness.draw();
    harness.assert_contains("A thread-safe reference-counted pointer.");
    harness.assert_contains("current version docs");
    assert!(!harness.text().contains("No definition"), "{}", harness.text());
    Ok(())
}

#[test]
fn ctrl_hover_finds_names_on_a_wrapped_segment() -> Result<(), Box<dyn Error>> {
    let padding = "0, ".repeat(24);
    let source = format!("fn main() {{\n    let values = [{padding}]; double(2);\n}}\nfn double(value: u32) {{}}\n");
    let document = DocumentBuilder::new().changed("src/main.rs", "", &source).build();
    let mut harness = ReviewHarness::new(document, 60, 20);
    harness.draw();
    let (x, y) = name_at(&harness, "]; double(2)", "double")?;
    assert_ne!(Some(y), harness.row_containing("let values"));
    harness.input(mouse_with(MouseEventKind::Moved, x, y, KeyModifiers::CONTROL));
    assert_eq!(card_name(&harness).as_deref(), Some("double"));
    Ok(())
}

#[test]
fn the_card_opens_under_the_name_it_describes_on_its_own_side() -> Result<(), Box<dyn Error>> {
    let document = DocumentBuilder::new()
        .changed("src/lib.rs", "", "pub fn double(value: u32) -> u32 {\n    value * 2\n}\n")
        .changed("src/main.rs", "fn main() {\n    let total = 1;\n}\n", "fn main() {\n    let total = double(1);\n}\n")
        .build();
    let mut harness = ReviewHarness::new(document, 200, 30);
    harness.state_mut().set_view_mode(ViewMode::Split);
    harness.draw();
    let (x, y) = name_at(&harness, "let total = double(1);", "double")?;
    harness.input(mouse_with(MouseEventKind::Moved, x, y, KeyModifiers::CONTROL));
    harness.draw();
    let top = harness.row_text(y + 1);
    let corner = top.find('┌').ok_or_else(|| format!("the card sits right under its line:\n{}", harness.text()))?;
    assert_eq!(u16::try_from(top[..corner].chars().count())?, x, "{}", harness.text());
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
    let background = |name| -> Result<Option<Color>, Box<dyn Error>> { Ok(name_style(harness, TOTAL, name)?.bg) };
    Ok([background("double")?, background("add")?, background("total")?])
}

fn underlined(harness: &ReviewHarness, line: &str, name: &str) -> Result<bool, Box<dyn Error>> {
    Ok(name_style(harness, line, name)?.add_modifier.contains(Modifier::UNDERLINED))
}

fn name_style(harness: &ReviewHarness, line: &str, name: &str) -> Result<Style, Box<dyn Error>> {
    let (x, y) = name_at(harness, line, name)?;
    Ok(harness.buffer().cell((x, y)).ok_or("cell is off screen")?.style())
}

fn name_at(harness: &ReviewHarness, line: &str, name: &str) -> Result<(u16, u16), Box<dyn Error>> {
    let position = harness.position_in(line, name).ok_or("the name is not visible on its line")?;
    Ok((position.x, position.y))
}

fn card_name(harness: &ReviewHarness) -> Option<String> {
    harness.state().definition_card().map(|card| card.name.clone())
}

fn assert_meta(harness: &ReviewHarness, place: &str, uses: &str) -> Result<(), Box<dyn Error>> {
    let row = harness.row_containing(uses).ok_or("the card shows its uses")?;
    let text = harness.row_text(row);
    let (place_at, uses_at) = (text.find(place).ok_or("the card shows its place")?, text.find(uses).unwrap_or(0));
    assert!(place_at < uses_at, "place and uses share the card's meta row:\n{}", harness.text());
    Ok(())
}

const TOTAL: &str = "let total = double(add(1, 2));";

const STD: &str = "std · alloc/src/sync.rs";
const MAIN: &str = "fn main() {\n    let total = double(add(1, 2));\n    double(total);\n    missing();\n}\n";
