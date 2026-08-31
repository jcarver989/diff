#![allow(clippy::unwrap_used)]

use crossterm::event::{KeyCode, MouseEventKind};
use diff_markdown::{
    MarkdownDocument, MarkdownReviewDecision, MarkdownReviewEvent, MarkdownTargetKind,
};
use diff_ratatui::MarkdownReviewState;
use diff_ratatui::testing::{
    markdown_key as key, markdown_mouse as mouse, render_markdown_review as draw,
    type_markdown_text as type_text,
};
use std::sync::Arc;

fn document() -> Arc<MarkdownDocument> {
    Arc::new(MarkdownDocument::parse_with_metadata(
        Some("plan.md".to_owned()),
        Some("Plan".to_owned()),
        "# Plan\n\nA **paragraph** with `code` and [a link](https://example.com).\n\n- first\n  - nested\n\n> quoted text\n\n| Name | Value |\n| --- | --- |\n| one | two |\n\n```rust\nlet value = 1;\nprintln!(\"{value}\");\n```\n",
    ))
}

#[test]
fn renders_formatted_markdown_without_source_delimiters() {
    let mut state = MarkdownReviewState::new(document());
    let rendered = draw(&mut state, 100, 24);
    assert!(rendered.contains("# Plan"), "{rendered}");
    assert!(rendered.contains("paragraph"), "{rendered}");
    assert!(rendered.contains("first"), "{rendered}");
    assert!(rendered.contains("quoted text"), "{rendered}");
    assert!(rendered.contains("Name"), "{rendered}");
    assert!(rendered.contains("let value = 1;"), "{rendered}");
    assert!(!rendered.contains("```"), "{rendered}");
    assert!(!rendered.contains("[a link]"), "{rendered}");
}

#[test]
fn renders_source_line_gutters_and_preserves_blank_lines_between_blocks() {
    let mut state = MarkdownReviewState::new(document());
    let rendered = draw(&mut state, 80, 30);

    assert!(rendered.contains(" 1 │ # Plan"), "{rendered}");
    assert!(rendered.contains(" 2 │ "), "{rendered}");
    assert!(
        rendered.contains(" 3 │ A paragraph with code and a link."),
        "{rendered}"
    );
    assert!(rendered.contains("15 │ let value = 1;"), "{rendered}");
    assert!(rendered.contains("16 │ println!"), "{rendered}");

    let lines = rendered.lines().collect::<Vec<_>>();
    let heading = lines
        .iter()
        .position(|line| line.contains(" 1 │ # Plan"))
        .unwrap();
    assert!(lines[heading + 1].contains(" 2 │ "), "{rendered}");
    assert!(
        lines[heading + 2].contains(" 3 │ A paragraph"),
        "{rendered}"
    );
}

#[test]
fn target_navigation_and_code_lines_are_semantic() {
    let mut state = MarkdownReviewState::new(document());
    assert_eq!(state.selected_target().unwrap().index(), 0);
    assert_eq!(
        state.session().selected_target_info().unwrap().kind,
        MarkdownTargetKind::Heading
    );
    state.handle_input(key(KeyCode::Down)).unwrap();
    assert_eq!(
        state.session().selected_target_info().unwrap().kind,
        MarkdownTargetKind::Paragraph
    );
    while state.session().selected_target_info().unwrap().kind != MarkdownTargetKind::CodeLine {
        state.handle_input(key(KeyCode::Down)).unwrap();
    }
    assert_eq!(
        state.session().selected_target_info().unwrap().kind,
        MarkdownTargetKind::CodeLine
    );
    assert_eq!(
        state
            .session()
            .selected_target_info()
            .unwrap()
            .display_label,
        "Code line 1"
    );
}

#[test]
fn the_wheel_moves_one_selected_markdown_target_per_notch() {
    let mut keyboard = MarkdownReviewState::new(document());
    let first = keyboard.selected_target();
    keyboard.handle_input(key(KeyCode::Down)).unwrap();
    let next = keyboard.selected_target();

    let mut state = MarkdownReviewState::new(document());
    draw(&mut state, 100, 12);
    assert_eq!(state.selected_target(), first);

    state
        .handle_input(mouse(MouseEventKind::ScrollDown, 60, 5))
        .unwrap();
    draw(&mut state, 100, 12);
    assert_eq!(state.selected_target(), next);

    state
        .handle_input(mouse(MouseEventKind::ScrollUp, 60, 5))
        .unwrap();
    draw(&mut state, 100, 12);
    assert_eq!(state.selected_target(), first);
}

#[test]
fn comments_and_explicit_decisions_are_emitted() {
    let mut state = MarkdownReviewState::new(document());
    state.handle_input(key(KeyCode::Char('c'))).unwrap();
    type_text(&mut state, "Please revise");
    state.handle_input(key(KeyCode::Enter)).unwrap();
    assert_eq!(state.review().len(), 1);
    assert!(draw(&mut state, 80, 12).contains("Please revise"));

    let approved = state.handle_input(key(KeyCode::Char('a'))).unwrap();
    assert!(
        matches!(approved, Some(MarkdownReviewEvent::Submit(submission)) if submission.decision == MarkdownReviewDecision::Approved)
    );
    let requested = state.handle_input(key(KeyCode::Char('r'))).unwrap();
    assert!(
        matches!(requested, Some(MarkdownReviewEvent::Submit(submission)) if submission.decision == MarkdownReviewDecision::ChangesRequested)
    );
}

#[test]
fn outline_click_selects_heading_and_escape_cancels_draft_first() {
    let mut state = MarkdownReviewState::new(document());
    draw(&mut state, 120, 20);
    state
        .handle_input(mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            5,
            3,
        ))
        .unwrap();
    assert_eq!(
        state.session().selected_target_info().unwrap().kind,
        MarkdownTargetKind::Heading
    );
    state.handle_input(key(KeyCode::Char('c'))).unwrap();
    assert!(state.session().draft().is_some());
    assert_eq!(state.handle_input(key(KeyCode::Esc)).unwrap(), None);
    assert!(state.session().draft().is_none());
}
