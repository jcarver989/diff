#![allow(missing_docs, unused_must_use)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use diff_core::{
    DiffDocument, DiffReviewEvent, FileDiff, FileStatus, LineAnchor, PatchLine, PatchLineKind,
    RepoPath, Review, StageState, ViewMode,
};
use diff_ratatui::{DiffReviewInput, DiffReviewState, DiffReviewWidget, FocusPane};
use ratatui::{Terminal, backend::TestBackend, layout::Position};
use std::{fmt::Write, sync::Arc};

struct DocumentBuilder {
    files: Vec<FileDiff>,
}

impl DocumentBuilder {
    fn new() -> Self {
        Self { files: Vec::new() }
    }

    fn changed(mut self, path: &str, old: &str, new: &str) -> Self {
        self.files
            .push(FileDiff::from_texts(path, old, new).expect("valid fixture"));
        self
    }

    fn binary(mut self, path: &str) -> Self {
        self.files.push(FileDiff {
            old_path: Some(RepoPath::new(path).expect("valid fixture path")),
            path: RepoPath::new(path).expect("valid fixture path"),
            status: FileStatus::Modified,
            staged: StageState::Unstaged,
            hunks: Vec::new(),
            binary: true,
            mode: None,
            no_newline_at_end: false,
        });
        self
    }

    fn build(self) -> Arc<DiffDocument> {
        Arc::new(DiffDocument {
            repo_root: "/repo".to_owned(),
            files: self.files,
        })
    }
}

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

fn key(code: KeyCode) -> DiffReviewInput {
    DiffReviewInput::Key(KeyEvent::new(code, KeyModifiers::NONE))
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
    assert_eq!(state.presentation().view_mode(), ViewMode::Unified);
    assert!(!narrow.contains("☐ M"), "narrow view must hide the drawer");

    let wide = draw(&mut state, 140, 12);
    assert!(wide.contains("☐ M src/main.rs"), "{wide}");
    assert!(wide.contains('│'), "{wide}");
    assert_eq!(state.presentation().view_mode(), ViewMode::Split);
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

    let submitted = state.handle_input(key(KeyCode::Char('s')));
    let [DiffReviewEvent::SubmitReview(submission)] = submitted.as_slice() else {
        panic!("expected review submission: {submitted:?}");
    };
    assert_eq!(submission.comments.len(), 1);
    assert!(submission.formatted.contains("please fix 界"));

    let copied = state.handle_input(key(KeyCode::Char('y')));
    assert!(
        matches!(copied.as_slice(), [DiffReviewEvent::CopyFormattedReview(text)] if text.contains("please fix"))
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
        [DiffReviewEvent::Cancel]
    );
}

#[test]
fn document_replacement_retains_outdated_comments() {
    let before = changed_document();
    let file = &before.files[0];
    let anchor = LineAnchor::for_line(file, diff_core::DiffSide::Old, 0, 0).expect("anchor");
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
    let document = (0..12).fold(DocumentBuilder::new(), |builder, index| {
        builder.changed(&format!("src/file_{index:02}.rs"), "old\n", "new\n")
    });
    let mut state = DiffReviewState::new(document.build());
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
    assert_eq!(state.presentation().view_mode(), ViewMode::Unified);

    state.handle_input(key(KeyCode::Char('v')));
    assert_eq!(state.view_mode(), ViewMode::Split);
    draw(&mut state, 60, 12);
    assert_eq!(state.presentation().view_mode(), ViewMode::Split);

    state.handle_input(key(KeyCode::Char('v')));
    assert_eq!(state.view_mode(), ViewMode::Auto);
    draw(&mut state, 60, 12);
    assert_eq!(state.presentation().view_mode(), ViewMode::Unified);

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
        text: "metadata".to_owned(),
        old_line_no: None,
        new_line_no: None,
        no_newline: false,
    });
    let document = Arc::new(DiffDocument {
        repo_root: "/repo".to_owned(),
        files: vec![file],
    });
    let mut state = DiffReviewState::new(document);
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
