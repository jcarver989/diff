#![allow(missing_docs)]

mod support;

use clankerdiff_core::{DiffDocument, ViewMode, testing::DocumentBuilder};
use clankerdiff_ratatui::{KeyCode, MouseEventKind, NavigationPane, ReviewOptions};
use clankerdiff_ui_core::ReviewTheme;
use rouge_syntax::{analyze, thread_work};
use std::{error::Error, fmt::Write, str, sync::Arc};
use support::{MarkdownStreamFixture, ReviewHarness, key, mouse};

#[test]
fn source_view_large_snapshot_reuses_full_document_highlights() {
    let old = source_lines(100_000, "");
    let new = old.replace("let value_50000 = 50000;", "let changed = 50000;");
    let document = DocumentBuilder::new()
        .changed_with_hunk_window("large.rs", &old, &new, 49_998..=50_002)
        .changed("unrelated.rs", "old", "new")
        .build();
    let mut harness = ReviewHarness::new(document, 100, 24);
    harness.state_mut().set_options(ReviewOptions {
        navigation: NavigationPane::Hidden,
        footer: false,
        ..Default::default()
    });
    harness.draw();
    harness.input_and_draw(key(KeyCode::Char('o')));
    assert!(harness.state().session().source_view().is_some());
    for _ in 0..3 {
        harness.input_and_draw(key(KeyCode::Char('G')));
        let settled = harness.draw();
        assert_eq!(settled.parses, 0);
        harness.input_and_draw(key(KeyCode::Char('o')));
        harness.input_and_draw(key(KeyCode::Char('o')));
    }
}

#[test]
fn settled_frame_emits_no_terminal_cells_and_reuses_highlights() {
    let mut harness = ReviewHarness::new(large_document(1_000), 100, 24);
    let cold = harness.draw();
    assert!(cold.backend.cells_drawn > 0);
    assert!(cold.parses > 0);

    let settled = harness.draw();
    assert_eq!(settled.backend.cells_drawn, 0);
    assert_eq!(settled.parses, 0);
}

#[test]
fn wrapped_cells_parse_each_side_once() {
    for split in [false, true] {
        let source = format!("{}\n", "identifier_".repeat(1000));
        let document =
            DocumentBuilder::new().changed("long.rs", &source, &source.replace("identifier", "replacement")).build();
        let mut harness = ReviewHarness::new(document, 80, 12);
        harness.state_mut().set_options(ReviewOptions {
            navigation: NavigationPane::Hidden,
            footer: false,
            ..Default::default()
        });
        harness.state_mut().set_view_mode(if split { ViewMode::Split } else { ViewMode::Unified });
        let first = harness.draw();
        assert!(first.parses <= 2);
        for _ in 0..5 {
            assert_eq!(harness.input_and_draw(key(KeyCode::PageDown)).parses, 0);
        }
        assert!(harness.state().scroll_offset() > 20);
        assert_eq!(harness.draw().parses, 0);
    }
}

#[test]
fn an_equal_content_snapshot_swap_reuses_every_highlight() {
    let old = source_lines(1_000, "");
    let new = source_lines(1_000, " + 1");
    let fixture = || DocumentBuilder::new().changed("src/large.rs", &old, &new).build();
    let mut harness = ReviewHarness::new(fixture(), 100, 24);
    harness.draw();
    assert_eq!(harness.draw().parses, 0);

    harness.state_mut().set_document(fixture());

    let swapped = harness.draw();
    assert_eq!(swapped.parses, 0, "highlights of unchanged sources must survive a snapshot swap");
}

#[test]
fn highlighting_work_is_bounded_by_the_viewport_not_document_size() {
    let mut small = ReviewHarness::new(large_document(1_000), 80, 20);
    let mut large = ReviewHarness::new(large_document(100_000), 80, 20);

    let small_frame = small.draw();
    let large_frame = large.draw();
    assert_eq!(small_frame.parses, large_frame.parses);
    assert_eq!(small_frame.parsed_bytes, large_frame.parsed_bytes);
    assert!(
        large_frame.parsed_bytes < 100_000,
        "parsed {} bytes from a 100,000-line document",
        large_frame.parsed_bytes
    );
}

#[test]
fn deep_full_file_scroll_parses_complete_sources_once() {
    let old = source_lines(100_000, "");
    let new = old.replacen("let value_50000 = 50000;", "let value_50000 = 50000 + 1;", 1);
    let fixture = DocumentBuilder::new().changed_with_hunk_window("src/large.rs", &old, &new, 49_997..=50_003).build();
    let mut harness = ReviewHarness::new(fixture, 100, 24);
    harness.input(key(KeyCode::Tab));
    harness.input(key(KeyCode::Char('f')));
    let first_parse = harness.draw();
    assert!(
        first_parse.parsed_bytes >= old.len(),
        "first visible complete side parsed {} bytes",
        first_parse.parsed_bytes
    );
    let before = thread_work().parser_input_bytes;
    let _ = [&old, &new].map(|source| analyze("rust", source));
    let reference = thread_work().parser_input_bytes - before;
    assert!(first_parse.parses <= 2);
    assert!(first_parse.parsed_bytes <= reference);
    assert!(reference <= 4 * (old.len() + new.len()) + 64 * 1024);
    let jumped = harness.input_and_draw(key(KeyCode::End));
    assert!(jumped.parsed_bytes < 100_000, "deep full-file navigation parsed {} bytes", jumped.parsed_bytes);
    let settled = harness.draw();
    assert_eq!(settled.parses, 0);
}

#[test]
fn deep_patch_only_split_scroll_highlights_visible_lines_and_then_settles() {
    let mut harness = ReviewHarness::new(modified_document(5_000), 100, 24);
    harness.draw();
    harness.input(key(KeyCode::Enter));
    for _ in 0..3 {
        if harness.state().layout().is_split() {
            break;
        }
        harness.input(key(KeyCode::Char('v')));
    }
    assert!(harness.state().layout().is_split());
    harness.draw();

    for _ in 0..60 {
        harness.input(key(KeyCode::PageDown));
    }
    let jumped = harness.draw();
    assert!(
        jumped.parsed_bytes < 150_000,
        "a deep jump parses bounded context per side, parsed {} bytes",
        jumped.parsed_bytes
    );

    let settled = harness.draw();
    assert_eq!(settled.parses, 0, "the current patch-only viewport stays highlighted");
    let paged = harness.input_and_draw(key(KeyCode::PageDown));
    assert!(paged.parses <= 2, "a page parses at most one new window per side");
}

#[test]
fn moving_one_row_changes_only_a_bounded_screen_region() {
    const WIDTH: u16 = 100;
    let mut harness = ReviewHarness::new(large_document(10_000), WIDTH, 24);
    harness.draw();
    harness.input(key(KeyCode::Enter));
    harness.draw();

    let moved = harness.input_and_draw(key(KeyCode::Down));
    assert!(
        moved.backend.cells_drawn <= u64::from(WIDTH) * 3,
        "one-row navigation changed {} cells",
        moved.backend.cells_drawn
    );
    assert_eq!(moved.parses, 0);
}

#[test]
fn one_wheel_notch_scrolls_without_moving_selection() {
    const WIDTH: u16 = 100;
    const HEIGHT: u16 = 24;
    const PATCH_COLUMN: u16 = 60;
    let mut harness = ReviewHarness::new(large_document(10_000), WIDTH, HEIGHT);
    harness.draw();
    harness.input(key(KeyCode::Enter));
    harness.draw();
    let selected = harness.state().selected_row();

    let moved = harness.input_and_draw(mouse(MouseEventKind::ScrollDown, PATCH_COLUMN, 5));
    assert!(
        moved.backend.cells_drawn <= u64::from(WIDTH) * u64::from(HEIGHT),
        "one wheel notch changed {} cells",
        moved.backend.cells_drawn
    );
    assert_eq!(harness.state().selected_row(), selected);
    assert!(moved.parses <= 2);
}

#[test]
fn moving_back_to_a_shown_row_does_not_reparse_it() {
    let mut harness = ReviewHarness::new(large_document(1_000), 80, 20);
    harness.draw();
    harness.input(key(KeyCode::Enter));
    harness.draw();
    harness.input_and_draw(key(KeyCode::Down));

    let back = harness.input_and_draw(key(KeyCode::Up));
    assert_eq!(back.parses, 0);
    assert!(harness.state().selected_row().is_some());
    assert_ne!(harness.buffer().content(), []);
}

#[test]
fn changing_the_review_theme_does_not_reparse_visible_sources() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarness::new(large_document(1_000), 100, 24);
    assert!(harness.draw().parsed_bytes > 0);
    harness.state_mut().set_theme(ReviewTheme::ayu()?);
    let changed = harness.draw();
    assert_eq!(changed.parsed_bytes, 0);
    assert_eq!(changed.parses, 0);
    assert!(changed.backend.cells_drawn > 0);
    Ok(())
}

#[test]
fn aether_streaming_prose_has_bounded_parser_work() -> Result<(), Box<dyn Error>> {
    let mut source = String::new();
    let mut sentence = 0;
    while source.len() < 16 * 1024 {
        for _ in 0..4 {
            write!(source, "Sentence {sentence} carries ordinary words so wrapping and parsing do real work. ")?;
            sentence += 1;
        }
        source.push_str("\n\n");
    }
    assert_streaming_budget(&source, false)
}

#[test]
fn aether_streaming_code_has_bounded_parser_and_highlight_work() -> Result<(), Box<dyn Error>> {
    let mut source = String::from("```rust\n");
    let mut line = 0;
    while source.len() < 24 * 1024 {
        writeln!(source, "let value_{line} = state.reconcile(incoming[{line}]).expect(\"delta accepted\");")?;
        line += 1;
    }
    source.push_str("```\n");
    assert_streaming_budget(&source, true)
}

fn assert_streaming_budget(source: &str, code: bool) -> Result<(), Box<dyn Error>> {
    let mut fixture = MarkdownStreamFixture::default();
    fixture.options.width = 120;
    for chunk in source.as_bytes().chunks(256) {
        fixture.stream.push(str::from_utf8(chunk)?);
        fixture.apply_update();
    }
    fixture.stream.finish();
    fixture.apply_update();
    let work = fixture.state.take_stats();
    let budget = 4 * source.len() + 64 * 1024;
    assert_eq!(work.rows_materialized, 0);
    assert!(work.parsed_bytes <= budget && (!code || work.highlighted_bytes <= budget), "{work:?}, budget {budget}");
    fixture.assert_equivalent();
    fixture.state.take_stats();
    fixture.apply_update();
    let settled = fixture.state.take_stats();
    assert_eq!(settled.parsed_bytes, 0);
    assert_eq!(settled.highlighted_bytes, 0);
    assert_eq!(settled.rows_generated, 0);
    Ok(())
}

fn large_document(rows: usize) -> Arc<DiffDocument> {
    DocumentBuilder::new().generated("src/large.rs", rows).build()
}

fn modified_document(rows: usize) -> Arc<DiffDocument> {
    DocumentBuilder::new().changed("src/large.rs", &source_lines(rows, ""), &source_lines(rows, " + 1")).build()
}

fn source_lines(rows: usize, suffix: &str) -> String {
    (1..=rows).fold(String::new(), |mut source, i| {
        let _ = writeln!(source, "let value_{i} = {i}{suffix};");
        source
    })
}
