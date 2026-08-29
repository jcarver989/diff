#![allow(missing_docs)]

mod support;

use crossterm::event::{KeyCode, MouseEventKind};
use diff_core::{DiffDocument, testing::DocumentBuilder};
use std::sync::Arc;
use support::{ReviewHarness, key, mouse};

fn large_document(rows: usize) -> Arc<DiffDocument> {
    DocumentBuilder::new()
        .generated("src/large.rs", rows)
        .build()
}

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
fn highlighting_work_is_bounded_by_the_viewport_not_document_size() {
    let mut small = ReviewHarness::new(large_document(1_000), 80, 20);
    let mut large = ReviewHarness::new(large_document(100_000), 80, 20);

    let small_frame = small.draw();
    let large_frame = large.draw();
    assert_eq!(small_frame.highlight_calls, large_frame.highlight_calls);
    assert_eq!(small_frame.highlight_misses, large_frame.highlight_misses);
    assert!(large_frame.highlight_calls <= 18);
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
fn one_wheel_notch_changes_only_a_bounded_screen_region() {
    const WIDTH: u16 = 100;
    const PATCH_COLUMN: u16 = 60;
    let mut harness = ReviewHarness::new(large_document(10_000), WIDTH, 24);
    harness.draw();
    harness.input(key(KeyCode::Enter));
    harness.draw();

    let moved = harness.input_and_draw(mouse(MouseEventKind::ScrollDown, PATCH_COLUMN, 5));
    assert!(
        moved.backend.cells_drawn <= u64::from(WIDTH) * 3,
        "one wheel notch changed {} cells",
        moved.backend.cells_drawn
    );
    assert_eq!(moved.highlight_misses, 0);
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
