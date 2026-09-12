#![allow(missing_docs)]

mod support;

use clankerdiff_core::{DiffDocument, ViewMode, testing::DocumentBuilder};
use clankerdiff_ratatui::{KeyCode, MouseEventKind, NavigationPane, ReviewOptions};
use clankerdiff_syntax::{Fingerprint, SyntaxError, SyntaxHighlighter, SyntaxTheme};
use clankerdiff_theme::ReviewTheme;
use std::{error::Error, fmt::Write, str, sync::Arc};
use support::{MarkdownStreamFixture, ReviewHarness, key, mouse};

#[test]
fn settled_frame_emits_no_terminal_cells_and_reuses_highlights() {
    let mut harness = ReviewHarness::new(large_document(1_000), 100, 24);
    let cold = harness.draw();
    assert!(cold.backend.cells_drawn > 0);
    assert!(cold.highlight_misses > 0);

    let settled = harness.draw();
    assert_eq!(settled.backend.cells_drawn, 0);
    assert_eq!(settled.highlight_misses, 0);
    assert_eq!(settled.highlight_calls, settled.highlight_hits);
}

#[test]
fn wrapped_cells_are_highlighted_once_per_visible_side() {
    for split in [false, true] {
        let source = format!("{}\n", "identifier_".repeat(1000));
        let document = DocumentBuilder::new()
            .changed(
                "long.rs",
                &source,
                &source.replace("identifier", "replacement"),
            )
            .build();
        let mut harness = ReviewHarness::new(document, 80, 12);
        harness.state_mut().set_options(ReviewOptions {
            navigation: NavigationPane::Hidden,
            footer: false,
            ..Default::default()
        });
        harness.state_mut().set_view_mode(if split {
            ViewMode::Split
        } else {
            ViewMode::Unified
        });
        harness.draw();
        for _ in 0..5 {
            harness.input_and_draw(key(KeyCode::PageDown));
        }
        assert!(harness.state().scroll_offset() > 20);
        let settled = harness.draw();
        assert_eq!(settled.highlight_calls, if split { 2 } else { 1 });
    }
}

#[test]
fn an_equal_content_snapshot_swap_reuses_every_highlight() {
    let old = source_lines(1_000, "");
    let new = source_lines(1_000, " + 1");
    let fixture = || {
        DocumentBuilder::new()
            .changed("src/large.rs", &old, &new)
            .build()
    };
    let mut harness = ReviewHarness::new(fixture(), 100, 24);
    harness.draw();
    assert_eq!(harness.draw().highlight_misses, 0);

    harness.state_mut().set_document(fixture());

    let swapped = harness.draw();
    assert_eq!(
        swapped.highlight_misses, 0,
        "content-addressed highlights must survive a snapshot swap"
    );
    assert_eq!(swapped.highlight_calls, swapped.highlight_hits);
}

#[test]
fn highlighting_work_is_bounded_by_the_viewport_not_document_size() {
    let mut small = ReviewHarness::new(large_document(1_000), 80, 20);
    let mut large = ReviewHarness::new(large_document(100_000), 80, 20);

    let small_frame = small.draw();
    let large_frame = large.draw();
    assert_eq!(small_frame.highlight_calls, large_frame.highlight_calls);
    assert_eq!(small_frame.highlight_misses, large_frame.highlight_misses);
    assert!(large_frame.highlight_calls <= 18);
    assert_eq!(small_frame.highlighted_bytes, large_frame.highlighted_bytes);
    assert!(
        large_frame.highlighted_bytes < 100_000,
        "highlighted {} bytes from a 100,000-line document",
        large_frame.highlighted_bytes
    );
}

#[test]
fn deep_full_file_scroll_parses_complete_sources_once() -> Result<(), SyntaxError> {
    let old = source_lines(100_000, "");
    let new = old.replacen(
        "let value_50000 = 50000;",
        "let value_50000 = 50000 + 1;",
        1,
    );
    let fixture = DocumentBuilder::new()
        .changed_with_hunk_window("src/large.rs", &old, &new, 49_997..=50_003)
        .build();
    let mut harness = ReviewHarness::new(fixture, 100, 24);
    harness.input(key(KeyCode::Tab));
    harness.input(key(KeyCode::Char('f')));
    let first_parse = harness.draw();
    assert!(
        first_parse.highlighted_bytes >= old.len(),
        "first visible complete side parsed {} bytes",
        first_parse.highlighted_bytes
    );
    let mut reference = SyntaxHighlighter::new(0);
    let theme = SyntaxTheme::default();
    for source in [&old, &new] {
        reference.with_theme(&theme).highlight_document(
            Fingerprint::of([source.as_str()]),
            "rust",
            || source,
        )?;
    }
    assert!(first_parse.highlight_misses <= 2);
    assert!(first_parse.highlighted_bytes <= reference.stats().bytes);
    assert!(reference.stats().bytes <= 4 * (old.len() + new.len()) + 64 * 1024);
    let jumped = harness.input_and_draw(key(KeyCode::End));
    assert!(
        jumped.highlighted_bytes < 100_000,
        "deep full-file navigation parsed {} bytes",
        jumped.highlighted_bytes
    );
    let settled = harness.draw();
    assert_eq!(settled.highlight_misses, 0);
    Ok(())
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
        jumped.highlighted_bytes < 150_000,
        "a deep jump parses bounded context per side, parsed {} bytes",
        jumped.highlighted_bytes
    );

    let settled = harness.draw();
    assert_eq!(
        settled.highlight_misses, 0,
        "the current patch-only viewport stays cached"
    );
    let paged = harness.input_and_draw(key(KeyCode::PageDown));
    assert!(
        paged.highlight_misses <= 48,
        "patch-only fallback highlights at most the newly visible split cells"
    );
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
    assert_eq!(moved.highlight_misses, 0);
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
    assert!(moved.highlight_misses <= 2);
}

#[test]
fn moving_back_to_a_cached_row_does_not_rehighlight_it() {
    let mut harness = ReviewHarness::new(large_document(1_000), 80, 20);
    harness.draw();
    harness.input(key(KeyCode::Enter));
    harness.draw();
    harness.input_and_draw(key(KeyCode::Down));

    let back = harness.input_and_draw(key(KeyCode::Up));
    assert_eq!(back.highlight_misses, 0);
    assert_eq!(back.highlight_calls, back.highlight_hits);
    assert!(harness.state().selected_row().is_some());
    assert!(!harness.buffer().content().is_empty());
}

#[test]
fn changing_the_review_theme_does_not_reparse_visible_sources() -> Result<(), Box<dyn Error>> {
    let mut harness = ReviewHarness::new(large_document(1_000), 100, 24);
    assert!(harness.draw().highlighted_bytes > 0);
    harness.state_mut().set_theme(ReviewTheme::ayu()?);
    let changed = harness.draw();
    assert_eq!(changed.highlighted_bytes, 0);
    assert_eq!(changed.highlight_misses, 0);
    assert!(changed.highlight_hits > 0);
    assert!(changed.backend.cells_drawn > 0);
    Ok(())
}

#[test]
fn aether_streaming_prose_has_bounded_parser_work() -> Result<(), Box<dyn Error>> {
    let mut source = String::new();
    let mut sentence = 0;
    while source.len() < 16 * 1024 {
        for _ in 0..4 {
            write!(
                source,
                "Sentence {sentence} carries ordinary words so wrapping and parsing do real work. "
            )?;
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
        writeln!(
            source,
            "let value_{line} = state.reconcile(incoming[{line}]).expect(\"delta accepted\");"
        )?;
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
    assert!(
        work.parsed_bytes <= budget && (!code || work.highlighted_bytes <= budget),
        "{work:?}, budget {budget}"
    );
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
    DocumentBuilder::new()
        .generated("src/large.rs", rows)
        .build()
}

fn modified_document(rows: usize) -> Arc<DiffDocument> {
    DocumentBuilder::new()
        .changed(
            "src/large.rs",
            &source_lines(rows, ""),
            &source_lines(rows, " + 1"),
        )
        .build()
}

fn source_lines(rows: usize, suffix: &str) -> String {
    (1..=rows).fold(String::new(), |mut source, i| {
        let _ = writeln!(source, "let value_{i} = {i}{suffix};");
        source
    })
}
