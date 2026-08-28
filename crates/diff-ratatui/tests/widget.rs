#![allow(missing_docs, unused_must_use)]

mod support;

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use diff_core::{
    DiffDocument, DiffReviewEvent, DiffSide, FileDiff, Layout, LineAnchor, PatchLine,
    PatchLineKind, Review, ViewMode, testing::DocumentBuilder,
};
use diff_ratatui::{DiffReviewInput, DiffReviewState, DiffReviewWidget, FocusPane};
use ratatui::{Terminal, backend::TestBackend, layout::Position};
use std::{fmt::Write, sync::Arc};
use support::{key, key_with, mouse};

fn changed_document() -> Arc<DiffDocument> {
    DocumentBuilder::new()
        .changed("src/main.rs", "fn old() {}\n", "fn new() {}\n")
        .changed("README.md", "old\n", "new\n")
        .build()
}

fn draw(state: &mut DiffReviewState, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| frame.render_stateful_widget(DiffReviewWidget::new(), frame.area(), state))
        .expect("draw widget");
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .enumerate()
        .fold(String::new(), |mut output, (index, cell)| {
            if index != 0 && index % usize::from(width) == 0 {
                output.push('\n');
            }
            output.push_str(cell.symbol());
            output
        })
}

fn type_text(state: &mut DiffReviewState, text: &str) {
    for character in text.chars() {
        state.handle_input(key(KeyCode::Char(character)));
    }
}

#[test]
fn narrow_and_wide_views_share_core_presentation() {
    let mut state = DiffReviewState::new(changed_document());
    let narrow = draw(&mut state, 60, 12);
    assert!(narrow.contains("src/main.rs"), "{narrow}");
    assert!(narrow.contains("fn old() {}"), "{narrow}");
    assert!(narrow.contains("fn new() {}"), "{narrow}");
    assert_eq!(state.layout(), Layout::Unified);
    assert!(!narrow.contains("☐ M"), "narrow view must hide the drawer");

    let wide = draw(&mut state, 140, 12);
    assert!(wide.contains("☐ M src/main.rs"), "{wide}");
    assert!(wide.contains('│'), "{wide}");
    assert_eq!(state.layout(), Layout::Split);
}

#[test]
fn comment_crud_and_host_events_are_structured() {
    let mut state = DiffReviewState::new(changed_document());
    state.handle_input(key(KeyCode::Enter));
    assert_eq!(state.focus(), FocusPane::Diff);
    state.handle_input(key(KeyCode::Char('c')));
    type_text(&mut state, "please fix 界");
    state.handle_input(key(KeyCode::Enter));
    assert_eq!(state.review().len(), 1);
    assert_eq!(
        state.review().comments()[0].context.line_text,
        "fn old() {}"
    );

    let rendered = draw(&mut state, 100, 12);
    assert!(rendered.contains("please fix 界"), "{rendered}");

    let Some(DiffReviewEvent::SubmitReview(submission)) =
        state.handle_input(key(KeyCode::Char('s')))
    else {
        panic!("expected a review submission");
    };
    assert_eq!(submission.comments.len(), 1);
    assert!(submission.formatted.contains("please fix 界"));

    let copied = state.handle_input(key(KeyCode::Char('y')));
    assert!(
        matches!(copied, Some(DiffReviewEvent::CopyFormattedReview(ref text)) if text.contains("please fix"))
    );

    state.handle_input(key(KeyCode::Char('e')));
    state.handle_input(key(KeyCode::End));
    type_text(&mut state, " now");
    state.handle_input(key(KeyCode::Enter));
    assert_eq!(state.review().comments()[0].body, "please fix 界 now");
    state.handle_input(key(KeyCode::Char('x')));
    assert!(state.review().is_empty());

    assert_eq!(
        state.handle_input(key(KeyCode::Esc)),
        Some(DiffReviewEvent::Cancel)
    );
    assert_eq!(
        state.handle_input(key_with(KeyCode::Char('g'), KeyModifiers::CONTROL)),
        Some(DiffReviewEvent::Cancel)
    );
}

#[test]
fn split_view_can_comment_on_the_removed_side() {
    let mut state = DiffReviewState::new(changed_document());
    state.set_view_mode(ViewMode::Split);
    draw(&mut state, 140, 12);
    state.handle_input(key(KeyCode::Enter));

    state.handle_input(key(KeyCode::Left));
    assert_eq!(state.selected_side(), DiffSide::Old);
    state.handle_input(key(KeyCode::Char('c')));
    type_text(&mut state, "removed side");
    state.handle_input(key(KeyCode::Enter));

    state.handle_input(key(KeyCode::Right));
    assert_eq!(state.selected_side(), DiffSide::New);
    state.handle_input(key(KeyCode::Char('c')));
    type_text(&mut state, "added side");
    state.handle_input(key(KeyCode::Enter));

    let comments = state.review().comments();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].anchor.side, DiffSide::Old);
    assert_eq!(comments[0].context.line_text, "fn old() {}");
    assert_eq!(comments[1].anchor.side, DiffSide::New);
    assert_eq!(comments[1].context.line_text, "fn new() {}");

    state.handle_input(key(KeyCode::Char('h')));
    assert_eq!(state.focus(), FocusPane::Files);
}

#[test]
fn document_replacement_retains_outdated_comments() {
    let before = changed_document();
    let anchor = LineAnchor::for_line(&before.files[0], DiffSide::Old, 0, 0).expect("anchor");
    let mut review = Review::default();
    review.add_comment_with_context(anchor, "fn old() {}", "still relevant");
    let mut state = DiffReviewState::new(before);
    *state.review_mut() = review;

    state.set_document(
        DocumentBuilder::new()
            .changed("other.rs", "a\n", "b\n")
            .build(),
    );
    assert_eq!(state.review().len(), 1);
    assert!(state.review().comments()[0].outdated);
    let rendered = draw(&mut state, 80, 8);
    assert!(rendered.contains("1 comment (1 outdated)"), "{rendered}");
}

#[test]
fn loading_error_empty_and_binary_states_render() {
    let mut loading = DiffReviewState::loading();
    assert!(draw(&mut loading, 50, 6).contains("Loading diff"));
    loading.set_error("host failed");
    assert!(draw(&mut loading, 50, 6).contains("host failed"));
    loading.set_document(DocumentBuilder::new().build());
    assert!(draw(&mut loading, 50, 6).contains("No changes"));

    let mut binary = DiffReviewState::new(DocumentBuilder::new().binary("data.bin").build());
    assert!(draw(&mut binary, 50, 7).contains("Binary file changed"));
}

#[test]
fn paging_mouse_selection_and_visible_highlighting_are_bounded() {
    let old = (0..200).fold(String::new(), |mut output, line| {
        writeln!(output, "let old_{line} = {line};").expect("write fixture");
        output
    });
    let new = (0..200).fold(String::new(), |mut output, line| {
        writeln!(output, "let new_{line} = {line};").expect("write fixture");
        output
    });
    let document = DocumentBuilder::new()
        .changed("many.rs", &old, &new)
        .changed("second.rs", "old\n", "new\n")
        .build();
    let mut state = DiffReviewState::new(document);
    draw(&mut state, 80, 10);
    assert!(
        state.highlight_stats().calls <= 8,
        "only visible rows should highlight"
    );

    state.handle_input(key(KeyCode::Enter));
    let before = state.selected_row().expect("selected row");
    state.handle_input(key(KeyCode::PageDown));
    assert!(state.selected_row().expect("selected row") > before);
    assert!(state.scroll_offset() > 0);

    state.handle_input(DiffReviewInput::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 2,
        row: 2,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(state.focus(), FocusPane::Files);
    assert_eq!(state.selected_file(), Some(1));
}

#[test]
fn draft_editing_is_utf8_safe_and_escape_cancels() {
    let mut state = DiffReviewState::new(changed_document());
    state.handle_input(key(KeyCode::Enter));
    state.handle_input(key(KeyCode::Char('c')));
    type_text(&mut state, "a界");
    state.handle_input(key(KeyCode::Left));
    state.handle_input(key(KeyCode::Backspace));
    state.handle_input(key(KeyCode::Enter));
    assert_eq!(state.review().comments()[0].body, "界");

    state.handle_input(key(KeyCode::Char('c')));
    type_text(&mut state, "discard");
    state.handle_input(key(KeyCode::Esc));
    assert_eq!(state.review().len(), 1);
}

#[test]
fn draft_cursor_uses_terminal_display_width() {
    let mut state = DiffReviewState::new(changed_document());
    state.handle_input(key(KeyCode::Enter));
    state.handle_input(key(KeyCode::Char('c')));
    type_text(&mut state, "ab");

    draw(&mut state, 60, 12);
    let ascii = state.cursor_position().expect("visible draft cursor");
    assert_eq!(ascii, Position::new(5, 4));

    type_text(&mut state, "界");
    draw(&mut state, 60, 12);
    let wide = state.cursor_position().expect("visible wide draft cursor");
    assert_eq!(wide, Position::new(7, 4));

    state.handle_input(key(KeyCode::Left));
    draw(&mut state, 60, 12);
    assert_eq!(state.cursor_position(), Some(ascii));

    state.handle_input(key(KeyCode::Esc));
    assert_eq!(state.cursor_position(), None);
}

#[test]
fn file_drawer_scrolls_and_mouse_selection_uses_its_offset() {
    let mut state = DiffReviewState::new(DocumentBuilder::new().generated_files(12, 1).build());
    draw(&mut state, 100, 8);

    for _ in 0..8 {
        state.handle_input(key(KeyCode::Down));
    }
    assert_eq!(state.selected_file(), Some(8));
    let scrolled = draw(&mut state, 100, 8);
    assert!(scrolled.contains("src/file_08.rs"), "{scrolled}");
    assert!(!scrolled.contains("src/file_00.rs"), "{scrolled}");

    state.handle_input(DiffReviewInput::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 2,
        row: 1,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(state.selected_file(), Some(4));
}

#[test]
fn help_popup_and_explicit_view_mode_cycle_render() {
    let mut state = DiffReviewState::new(changed_document());
    assert_eq!(state.view_mode(), ViewMode::Auto);

    state.handle_input(key(KeyCode::Char('v')));
    assert_eq!(state.view_mode(), ViewMode::Unified);
    draw(&mut state, 140, 12);
    assert_eq!(state.layout(), Layout::Unified);

    state.handle_input(key(KeyCode::Char('v')));
    assert_eq!(state.view_mode(), ViewMode::Split);
    draw(&mut state, 60, 12);
    assert_eq!(state.layout(), Layout::Split);

    state.handle_input(key(KeyCode::Char('v')));
    assert_eq!(state.view_mode(), ViewMode::Auto);
    draw(&mut state, 60, 12);
    assert_eq!(state.layout(), Layout::Unified);

    state.handle_input(key(KeyCode::Char('?')));
    let help = draw(&mut state, 80, 16);
    assert!(help.contains("Review shortcuts"), "{help}");
    assert!(help.contains("Navigation"), "{help}");
    state.handle_input(key(KeyCode::Esc));
    let closed = draw(&mut state, 80, 16);
    assert!(!closed.contains("Navigation"), "{closed}");
}

#[test]
fn unusual_patch_metadata_does_not_become_commentable() {
    let mut file = FileDiff::from_texts("meta.txt", "old", "new").expect("fixture");
    file.hunks[0].lines.push(PatchLine {
        kind: PatchLineKind::Meta,
        text: "metadata".into(),
        old_line_no: None,
        new_line_no: None,
        no_newline: false,
    });
    let mut state = DiffReviewState::new(DocumentBuilder::new().file(file).build());
    state.handle_input(key(KeyCode::Enter));
    state.handle_input(key(KeyCode::End));
    state.handle_input(key(KeyCode::Char('c')));
    type_text(&mut state, "comment");
    state.handle_input(key(KeyCode::Enter));
    assert_eq!(
        state.review().len(),
        1,
        "End selects the last source row, not metadata"
    );
}

/// At 100 columns the bordered body gives the drawer columns 1..33 and the
/// patch everything past the separator at column 33.
const DRAWER_COLUMN: u16 = 5;
const PATCH_COLUMN: u16 = 60;

#[test]
fn the_wheel_scrolls_the_pane_under_the_pointer_and_leaves_the_selection_alone() {
    let mut state = DiffReviewState::new(DocumentBuilder::new().generated_files(40, 60).build());
    draw(&mut state, 100, 24);
    let selected_row = state.selected_row();
    assert_eq!(state.focus(), FocusPane::Files);

    // Pointing at the patch scrolls the patch, even though the drawer has focus.
    state.handle_input(mouse(MouseEventKind::ScrollDown, PATCH_COLUMN, 5));
    draw(&mut state, 100, 24);
    assert!(
        state.scroll_offset() > 0,
        "the wheel should scroll the patch"
    );
    assert_eq!(
        state.selected_row(),
        selected_row,
        "scrolling must not move the selection"
    );
    assert_eq!(state.selected_file(), Some(0), "no file switch");
    assert_eq!(
        state.focus(),
        FocusPane::Files,
        "scrolling must not steal focus"
    );

    // Scrolling back up returns the viewport without disturbing the selection.
    state.handle_input(mouse(MouseEventKind::ScrollUp, PATCH_COLUMN, 5));
    draw(&mut state, 100, 24);
    assert_eq!(state.scroll_offset(), 0);
    assert_eq!(state.selected_row(), selected_row);
}

#[test]
fn the_wheel_over_the_drawer_scrolls_one_file_at_a_time() {
    let mut state = DiffReviewState::new(DocumentBuilder::new().generated_files(40, 4).build());
    // The stage marker prefix distinguishes a drawer row from the patch header,
    // which keeps naming the selected file.
    let first = draw(&mut state, 100, 24);
    assert!(first.contains("\u{2610} A src/file_00.rs"), "{first}");

    state.handle_input(mouse(MouseEventKind::ScrollDown, DRAWER_COLUMN, 5));
    let scrolled = draw(&mut state, 100, 24);
    assert!(
        !scrolled.contains("\u{2610} A src/file_00.rs"),
        "{scrolled}"
    );
    assert!(scrolled.contains("\u{2610} A src/file_01.rs"), "{scrolled}");
    assert_eq!(
        state.selected_file(),
        Some(0),
        "scrolling the drawer must not select a different file"
    );
}

#[test]
fn events_that_change_nothing_do_not_ask_for_a_frame() {
    let mut state = DiffReviewState::new(changed_document());
    assert!(state.is_dirty(), "the first frame always draws");
    draw(&mut state, 100, 24);
    assert!(!state.is_dirty(), "a drawn frame settles the state");

    for kind in [
        MouseEventKind::Moved,
        MouseEventKind::Up(MouseButton::Left),
        MouseEventKind::Drag(MouseButton::Left),
    ] {
        state.handle_input(mouse(kind, PATCH_COLUMN, 5));
        assert!(!state.is_dirty(), "{kind:?} changes nothing that is drawn");
    }

    state.handle_input(mouse(MouseEventKind::ScrollUp, PATCH_COLUMN, 5));
    assert!(
        !state.is_dirty(),
        "the patch is already at the top of the file"
    );

    state.handle_input(key(KeyCode::Down));
    assert!(state.is_dirty(), "moving the selection needs a frame");
}

#[test]
fn keyboard_navigation_brings_a_scrolled_away_selection_back() {
    let mut state =
        DiffReviewState::new(DocumentBuilder::new().generated("src/long.rs", 400).build());
    draw(&mut state, 100, 24);
    state.handle_input(key(KeyCode::Enter));
    draw(&mut state, 100, 24);
    let selected = state.selected_row().expect("selected row");

    for _ in 0..20 {
        state.handle_input(mouse(MouseEventKind::ScrollDown, PATCH_COLUMN, 5));
    }
    draw(&mut state, 100, 24);
    assert!(
        state.scroll_offset() > selected,
        "the selection should be above the viewport"
    );

    state.handle_input(key(KeyCode::Down));
    draw(&mut state, 100, 24);
    let followed = state.selected_row().expect("selected row");
    assert!(
        state.scroll_offset() <= followed,
        "{followed} is off screen"
    );
}
