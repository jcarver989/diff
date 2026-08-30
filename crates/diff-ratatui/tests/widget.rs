#![allow(missing_docs, unused_must_use)]

mod support;

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use diff_core::{
    DiffDocument, DiffReviewEvent, DiffSide, FileDiff, Layout, LineAnchor, PatchLine,
    PatchLineKind, RepositoryAction, Review, StageState, ViewMode, testing::DocumentBuilder,
};
use diff_ratatui::{DiffReviewInput, DiffReviewState, DiffReviewWidget, FocusPane, RatatuiTheme};
use diff_theme::DiffTheme;
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
    assert!(!narrow.contains('☐'), "narrow view must hide the drawer");

    let wide = draw(&mut state, 140, 12);
    assert!(wide.contains("M main.rs +1 -1"), "{wide}");
    assert!(wide.contains("▾ src/"), "{wide}");
    assert!(wide.contains('│'), "{wide}");
    assert_eq!(state.layout(), Layout::Split);
}

#[test]
fn drawer_stage_markers_share_a_trailing_column() {
    let mut document = (*DocumentBuilder::new()
        .changed("src/nested/main.rs", "old\n", "new\n")
        .changed("src/lib.rs", "old\n", "new\n")
        .changed("README.md", "old\n", "new\n")
        .build())
    .clone();
    document.files[0].staged = StageState::Staged;
    document.files[1].staged = StageState::Unstaged;
    let mut state = DiffReviewState::new(Arc::new(document));

    let rendered = draw(&mut state, 100, 12);
    let marker_columns = rendered
        .lines()
        .filter_map(|line| {
            line.chars()
                .position(|character| matches!(character, '☐' | '☑' | '◩'))
        })
        .collect::<Vec<_>>();

    assert!(marker_columns.len() >= 5, "{rendered}");
    assert!(
        marker_columns
            .iter()
            .all(|column| *column == marker_columns[0]),
        "stage markers must share one column: {marker_columns:?}\n{rendered}"
    );
}

#[test]
fn trailing_drawer_stage_column_is_clickable_for_directories_and_files() {
    let document = DocumentBuilder::new()
        .changed("src/main.rs", "old\n", "new\n")
        .changed("src/lib.rs", "old\n", "new\n")
        .build();
    let expected_directory_paths = document
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    let mut state = DiffReviewState::new(document.clone());
    let rendered = draw(&mut state, 100, 8);
    let lines = rendered.lines().collect::<Vec<_>>();
    let directory_row = lines
        .iter()
        .position(|line| line.contains("▾ src/"))
        .expect("directory row");
    let stage_column = lines[directory_row]
        .chars()
        .position(|character| character == '☐')
        .expect("directory stage marker");

    assert_eq!(
        state.handle_input(mouse(
            MouseEventKind::Down(MouseButton::Left),
            u16::try_from(stage_column).unwrap(),
            u16::try_from(directory_row).unwrap(),
        )),
        Some(DiffReviewEvent::RepositoryAction(
            RepositoryAction::StagePaths(expected_directory_paths)
        ))
    );

    let mut state = DiffReviewState::new(document.clone());
    let rendered = draw(&mut state, 100, 8);
    let lines = rendered.lines().collect::<Vec<_>>();
    let file_row = lines
        .iter()
        .position(|line| line.contains("M main.rs"))
        .expect("file row");
    let stage_column = lines[file_row]
        .chars()
        .position(|character| character == '☐')
        .expect("file stage marker");

    assert_eq!(
        state.handle_input(mouse(
            MouseEventKind::Down(MouseButton::Left),
            u16::try_from(stage_column).unwrap(),
            u16::try_from(file_row).unwrap(),
        )),
        Some(DiffReviewEvent::RepositoryAction(
            RepositoryAction::StagePaths(vec![document.files[0].path.clone()])
        ))
    );
}

#[test]
fn changed_rows_use_semantic_gutter_bars_and_line_numbers() {
    let width = 60;
    let mut state = DiffReviewState::new(changed_document());
    let mut terminal = Terminal::new(TestBackend::new(width, 12)).expect("test terminal");
    terminal
        .draw(|frame| {
            frame.render_stateful_widget(DiffReviewWidget::new(), frame.area(), &mut state);
        })
        .expect("draw widget");

    let buffer = terminal.backend().buffer();
    let theme = RatatuiTheme::from(&DiffTheme::default());
    for (source, foreground) in [
        ("fn old() {}", theme.deletion),
        ("fn new() {}", theme.addition),
    ] {
        let row = buffer
            .content()
            .chunks(usize::from(width))
            .find(|row| {
                row.iter()
                    .map(ratatui::buffer::Cell::symbol)
                    .collect::<String>()
                    .contains(source)
            })
            .expect("changed source row");
        let indicator = row
            .iter()
            .position(|cell| cell.symbol() == "▌")
            .expect("changed-row indicator");
        assert_eq!(row[indicator].fg, foreground);
        assert_eq!(row[indicator + 4].symbol(), "1");
        assert_eq!(row[indicator + 4].fg, foreground);
    }
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
    assert_eq!(state.selected_row(), Some(before));
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
    assert_eq!(ascii, Position::new(7, 5));

    type_text(&mut state, "界");
    draw(&mut state, 60, 12);
    let wide = state.cursor_position().expect("visible wide draft cursor");
    assert_eq!(wide, Position::new(9, 5));

    state.handle_input(key(KeyCode::Left));
    draw(&mut state, 60, 12);
    assert_eq!(state.cursor_position(), Some(ascii));

    state.handle_input(key(KeyCode::Esc));
    assert_eq!(state.cursor_position(), None);
}

#[test]
fn comments_render_as_padded_themed_boxes() {
    let mut state = DiffReviewState::new(changed_document());
    let anchor = state.session().selected_anchor().expect("selected anchor");
    state
        .review_mut()
        .add_comment(anchor, "Please include the path in the error.");

    let rendered = draw(&mut state, 80, 12);

    assert!(rendered.contains("╭─ Comment "), "{rendered}");
    assert!(rendered.contains("│ Please include the path"), "{rendered}");
    assert!(rendered.contains('╰'), "{rendered}");

    let mut terminal = Terminal::new(TestBackend::new(80, 12)).expect("test terminal");
    terminal
        .draw(|frame| {
            frame.render_stateful_widget(DiffReviewWidget::new(), frame.area(), &mut state);
        })
        .expect("draw comment box");
    let border = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .find(|cell| cell.symbol() == "╭")
        .expect("top-left comment border");
    let expected = RatatuiTheme::from(&DiffTheme::default()).ui.accent;
    assert_eq!(border.fg, expected);
}

#[test]
fn tall_comment_boxes_can_be_paged_line_by_line() {
    let mut state = DiffReviewState::new(changed_document());
    let anchor = state.session().selected_anchor().expect("selected anchor");
    let body = (0..14)
        .map(|index| format!("comment line {index:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    state.review_mut().add_comment(anchor, body);
    let top = draw(&mut state, 80, 8);
    assert!(top.contains("comment line 00"), "{top}");
    assert!(!top.contains("comment line 13"), "{top}");

    for _ in 0..5 {
        state.handle_input(key(KeyCode::PageDown));
    }
    let bottom = draw(&mut state, 80, 8);

    assert!(bottom.contains("comment line 13"), "{bottom}");
    assert!(
        bottom.contains('╰'),
        "the closing border must be reachable: {bottom}"
    );
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
fn directory_entries_navigate_collapse_and_expand_without_losing_the_patch_file() {
    let document = DocumentBuilder::new()
        .changed("src/lib.rs", "old\n", "new\n")
        .changed("src/bin/main.rs", "old\n", "new\n")
        .changed("README.md", "old\n", "new\n")
        .build();
    let mut state = DiffReviewState::new(document);
    let initial = draw(&mut state, 100, 12);
    assert!(initial.contains("▾ src/"), "{initial}");
    assert!(initial.contains("▾ bin/"), "{initial}");
    assert!(initial.contains("main.rs"), "{initial}");

    state.handle_input(key(KeyCode::Up));
    assert_eq!(state.selected_file(), Some(1));
    state.handle_input(key(KeyCode::Up));
    assert_eq!(
        state.selected_file(),
        Some(1),
        "directory selection must leave the patch file unchanged"
    );
    state.handle_input(key(KeyCode::Left));
    let collapsed = draw(&mut state, 100, 12);
    assert!(collapsed.contains("▸ bin/"), "{collapsed}");
    assert!(!collapsed.contains("M main.rs +1 -1"), "{collapsed}");
    assert!(
        collapsed.contains("src/bin/main.rs"),
        "active patch remains visible"
    );

    state.handle_input(key(KeyCode::Enter));
    let expanded = draw(&mut state, 100, 12);
    assert!(expanded.contains("▾ bin/"), "{expanded}");
    assert!(expanded.contains("M main.rs +1 -1"), "{expanded}");
    assert_eq!(state.focus(), FocusPane::Files);
}

#[test]
fn initial_file_selection_is_scrolled_into_a_sorted_drawer() {
    let mut builder = DocumentBuilder::new().changed("z.rs", "old\n", "new\n");
    for index in 0..12 {
        builder = builder.changed(&format!("dir_{index:02}/file.rs"), "old\n", "new\n");
    }
    let mut state = DiffReviewState::new(builder.build());

    let rendered = draw(&mut state, 100, 8);

    assert_eq!(state.selected_file(), Some(0));
    assert!(rendered.contains("M z.rs +1 -1"), "{rendered}");
}

#[test]
fn document_replacement_expands_the_selected_files_ancestors() {
    let document = DocumentBuilder::new()
        .changed("src/lib.rs", "old\n", "new\n")
        .build();
    let mut state = DiffReviewState::new(document);
    draw(&mut state, 100, 8);
    state.handle_input(key(KeyCode::Up));
    state.handle_input(key(KeyCode::Left));
    assert!(draw(&mut state, 100, 8).contains("▸ src/"));

    state.set_document(
        DocumentBuilder::new()
            .changed("src/lib.rs", "older\n", "newer\n")
            .build(),
    );
    let replaced = draw(&mut state, 100, 8);
    assert!(replaced.contains("▾ src/"), "{replaced}");
    assert!(replaced.contains("M lib.rs +1 -1"), "{replaced}");
}

#[test]
fn diff_and_drawer_render_scrollbars_without_covering_content() {
    let mut state = DiffReviewState::new(DocumentBuilder::new().generated_files(40, 400).build());
    let top = draw(&mut state, 100, 24);
    assert!(
        top.contains('▲'),
        "drawer and patch tracks should have top arrows: {top}"
    );
    assert!(
        top.contains('▼'),
        "drawer and patch tracks should have bottom arrows: {top}"
    );
    assert!(top.contains("let file_00_rs_value_1"), "{top}");

    state.handle_input(key(KeyCode::Enter));
    let selected = state.selected_row();
    for _ in 0..10 {
        state.handle_input(mouse(MouseEventKind::ScrollDown, PATCH_COLUMN, 5));
    }
    let scrolled = draw(&mut state, 100, 24);
    assert_ne!(
        top, scrolled,
        "scrolling should move patch content and its thumb"
    );
    assert_ne!(state.selected_row(), selected);
    assert!(
        scrolled.contains('█') || scrolled.contains('║'),
        "{scrolled}"
    );
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
fn the_wheel_over_the_diff_moves_one_selected_row_per_notch() {
    let document = DocumentBuilder::new().generated_files(40, 60).build();
    let mut keyboard = DiffReviewState::new(document.clone());
    draw(&mut keyboard, 100, 24);
    keyboard.handle_input(key(KeyCode::Enter));
    let first_row = keyboard.selected_row();
    keyboard.handle_input(key(KeyCode::Down));
    let next_row = keyboard.selected_row();

    let mut state = DiffReviewState::new(document);
    draw(&mut state, 100, 24);
    assert_eq!(state.focus(), FocusPane::Files);
    assert_eq!(state.selected_row(), first_row);

    state.handle_input(mouse(MouseEventKind::ScrollDown, PATCH_COLUMN, 5));
    draw(&mut state, 100, 24);
    assert_eq!(state.selected_row(), next_row);
    assert_eq!(state.selected_file(), Some(0), "no file switch");
    assert_eq!(
        state.focus(),
        FocusPane::Files,
        "scrolling must not steal focus"
    );

    state.handle_input(mouse(MouseEventKind::ScrollUp, PATCH_COLUMN, 5));
    draw(&mut state, 100, 24);
    assert_eq!(state.selected_row(), first_row);
}

#[test]
fn the_wheel_over_the_drawer_scrolls_one_file_at_a_time() {
    let mut state = DiffReviewState::new(DocumentBuilder::new().generated_files(40, 4).build());
    // The status plus basename distinguishes a drawer row from the patch header,
    // which keeps naming the selected file.
    let first = draw(&mut state, 100, 24);
    assert!(first.contains("A file_00.rs"), "{first}");
    assert!(first.contains("▾ src/"), "{first}");

    state.handle_input(mouse(MouseEventKind::ScrollDown, DRAWER_COLUMN, 5));
    let one_entry = draw(&mut state, 100, 24);
    assert!(!one_entry.contains("▾ src/"), "{one_entry}");
    assert!(one_entry.contains("A file_00.rs"), "{one_entry}");

    state.handle_input(mouse(MouseEventKind::ScrollDown, DRAWER_COLUMN, 5));
    let scrolled = draw(&mut state, 100, 24);
    assert!(!scrolled.contains("A file_00.rs"), "{scrolled}");
    assert!(scrolled.contains("A file_01.rs"), "{scrolled}");
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
fn repository_actions_follow_file_and_directory_stage_state() {
    let document = changed_document();
    let mut state = DiffReviewState::new(document.clone());

    assert_eq!(
        state.handle_input(key(KeyCode::Char(' '))),
        Some(DiffReviewEvent::RepositoryAction(
            RepositoryAction::StagePaths(vec![document.files[0].path.clone(),])
        ))
    );
    assert_eq!(
        state.handle_input(key(KeyCode::Char('a'))),
        Some(DiffReviewEvent::RepositoryAction(
            RepositoryAction::StageAll
        ))
    );
    assert_eq!(
        state.handle_input(key(KeyCode::Char('A'))),
        Some(DiffReviewEvent::RepositoryAction(
            RepositoryAction::UnstageAll
        ))
    );

    let mut staged = (*document).clone();
    staged.files[0].staged = StageState::Staged;
    state.set_document(Arc::new(staged.clone()));
    assert_eq!(
        state.handle_input(key(KeyCode::Char(' '))),
        Some(DiffReviewEvent::RepositoryAction(
            RepositoryAction::UnstagePaths(vec![staged.files[0].path.clone(),])
        ))
    );
}

#[test]
fn commit_and_discard_require_explicit_input() {
    let mut document = (*changed_document()).clone();
    document.files[0].staged = StageState::Staged;
    let path = document.files[0].path.clone();
    let status = document.files[0].status;
    let mut state = DiffReviewState::new(Arc::new(document));

    state.handle_input(key(KeyCode::Char('C')));
    type_text(&mut state, "ship it");
    assert_eq!(
        state.handle_input(key(KeyCode::Enter)),
        Some(DiffReviewEvent::RepositoryAction(
            RepositoryAction::Commit {
                message: "ship it".to_owned(),
            }
        ))
    );

    state.handle_input(key(KeyCode::Char('d')));
    assert_eq!(
        state.handle_input(key(KeyCode::Char('y'))),
        Some(DiffReviewEvent::RepositoryAction(
            RepositoryAction::Discard { path, status }
        ))
    );
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
        state.handle_input(key(KeyCode::PageDown));
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
