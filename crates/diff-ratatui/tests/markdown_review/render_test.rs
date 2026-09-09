use clankerdiff_markdown::{
    MarkdownDocument, MarkdownReviewDecision, MarkdownReviewEvent, MarkdownTargetKind,
};
use clankerdiff_ratatui::testing::{
    key, mouse, render_markdown_review as draw, type_markdown_text as type_text,
};
use clankerdiff_ratatui::{
    InputOutcome, KeyCode, MarkdownReviewState, MouseButton, MouseEventKind,
};
use std::{error::Error, sync::Arc};

#[test]
fn renders_formatted_markdown_and_source_gutters() {
    let mut state = MarkdownReviewState::new(document());
    let rendered = draw(&mut state, 100, 30);
    for text in [
        " 1 │ # Plan",
        " 2 │ ",
        "A paragraph with code and a link.",
        "first",
        "quoted text",
        "Name",
        "15 │ let value = 1;",
        "16 │ println!",
    ] {
        assert!(rendered.contains(text), "{rendered}");
    }
    assert!(!rendered.contains("```"));
    assert!(!rendered.contains("[a link]"));
}

#[test]
fn target_navigation_and_code_lines_are_semantic() -> Result<(), Box<dyn Error>> {
    let mut state = MarkdownReviewState::new(document());
    assert_eq!(
        state
            .session()
            .selected_target_info()
            .ok_or("no target")?
            .kind,
        MarkdownTargetKind::Heading
    );
    state.handle_input(key(KeyCode::Down))?;
    assert_eq!(
        state
            .session()
            .selected_target_info()
            .ok_or("no target")?
            .kind,
        MarkdownTargetKind::Paragraph
    );
    for _ in 0..state.document().targets().len() {
        if state
            .session()
            .selected_target_info()
            .ok_or("no target")?
            .kind
            == MarkdownTargetKind::CodeLine
        {
            break;
        }
        state.handle_input(key(KeyCode::Down))?;
    }
    assert_eq!(
        state
            .session()
            .selected_target_info()
            .ok_or("no target")?
            .display_label,
        "Code line 1"
    );
    Ok(())
}

#[test]
fn wheel_matches_keyboard_navigation() -> Result<(), Box<dyn Error>> {
    let mut keyboard = MarkdownReviewState::new(document());
    let first = keyboard.selected_target();
    keyboard.handle_input(key(KeyCode::Down))?;
    let mut state = MarkdownReviewState::new(document());
    draw(&mut state, 100, 12);
    state.handle_input(mouse(MouseEventKind::ScrollDown, 60, 5))?;
    assert_eq!(state.selected_target(), keyboard.selected_target());
    draw(&mut state, 100, 12);
    state.handle_input(mouse(MouseEventKind::ScrollUp, 60, 5))?;
    assert_eq!(state.selected_target(), first);
    Ok(())
}

#[test]
fn comments_and_decisions_are_emitted() -> Result<(), Box<dyn Error>> {
    let mut state = MarkdownReviewState::new(document());
    state.handle_input(key(KeyCode::Char('c')))?;
    type_text(&mut state, "Please revise");
    state.handle_input(key(KeyCode::Enter))?;
    assert_eq!(state.review().len(), 1);
    assert!(draw(&mut state, 80, 12).contains("Please revise"));
    assert!(
        matches!(state.handle_input(key(KeyCode::Char('a')))?, InputOutcome::Emitted(MarkdownReviewEvent::Submit(submission)) if submission.decision == MarkdownReviewDecision::Approved)
    );
    assert!(
        matches!(state.handle_input(key(KeyCode::Char('r')))?, InputOutcome::Emitted(MarkdownReviewEvent::Submit(submission)) if submission.decision == MarkdownReviewDecision::ChangesRequested)
    );
    Ok(())
}

#[test]
fn outline_click_and_draft_cancellation() -> Result<(), Box<dyn Error>> {
    let mut state = MarkdownReviewState::new(document());
    draw(&mut state, 120, 20);
    state.handle_input(mouse(MouseEventKind::Down(MouseButton::Left), 5, 3))?;
    assert_eq!(
        state
            .session()
            .selected_target_info()
            .ok_or("no target")?
            .kind,
        MarkdownTargetKind::Heading
    );
    state.handle_input(key(KeyCode::Char('c')))?;
    assert!(state.session().draft().is_some());
    assert_eq!(
        state.handle_input(key(KeyCode::Esc))?,
        InputOutcome::Consumed
    );
    assert!(state.session().draft().is_none());
    Ok(())
}

fn document() -> Arc<MarkdownDocument> {
    Arc::new(MarkdownDocument::parse_with_metadata(
        Some("plan.md".to_owned()),
        Some("Plan".to_owned()),
        "# Plan\n\nA **paragraph** with `code` and [a link](https://example.com).\n\n- first\n  - nested\n\n> quoted text\n\n| Name | Value |\n| --- | --- |\n| one | two |\n\n```rust\nlet value = 1;\nprintln!(\"{value}\");\n```\n",
    ))
}
