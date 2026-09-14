use clankerdiff_markdown::{
    MarkdownDocument, MarkdownReviewDecision, MarkdownReviewEvent, MarkdownTargetKind,
};
use clankerdiff_ratatui::testing::{
    key, mouse, render_markdown_review as draw, type_markdown_text as type_text,
};
use clankerdiff_ratatui::{
    InputOutcome, KeyCode, MarkdownReviewState, MarkdownReviewWidget, MouseButton, MouseEventKind,
};
use clankerdiff_theme::ReviewTheme;
use ratatui::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};
use std::{error::Error, fmt::Write, sync::Arc};

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
fn wheel_keeps_navigating_at_a_fixed_position_through_source_gaps() -> Result<(), Box<dyn Error>> {
    let source = (0..30).fold(String::new(), |mut source, index| {
        let _ = writeln!(source, "# Heading {index}\n\nParagraph {index}");
        source
    });
    let document = Arc::new(MarkdownDocument::parse(&source));
    let mut keyboard = MarkdownReviewState::new(Arc::clone(&document));
    let mut state = MarkdownReviewState::new(document);
    let rendered = draw(&mut state, 100, 12);
    assert!(
        rendered
            .lines()
            .nth(2)
            .ok_or("missing gap row")?
            .contains("2 │")
    );
    draw(&mut keyboard, 100, 12);
    for (wheel, arrow) in [
        (MouseEventKind::ScrollDown, KeyCode::Down),
        (MouseEventKind::ScrollUp, KeyCode::Up),
    ] {
        for _ in 0..40 {
            assert_eq!(
                state.handle_input(mouse(wheel, 60, 2))?,
                InputOutcome::Consumed
            );
            keyboard.handle_input(key(arrow))?;
            draw(&mut state, 100, 12);
            draw(&mut keyboard, 100, 12);
            assert_eq!(state.selected_target(), keyboard.selected_target());
            assert_eq!(state.scroll_offset(), keyboard.scroll_offset());
        }
    }
    Ok(())
}

#[test]
fn wheel_accepts_blank_pane_space_and_scrollbar() -> Result<(), Box<dyn Error>> {
    for (column, row) in [(60, 2), (60, 8), (5, 8), (98, 2)] {
        let mut state = MarkdownReviewState::new(Arc::new(MarkdownDocument::parse(
            "# One\n\nText\n\n# Two\n",
        )));
        draw(&mut state, 100, 12);
        let selected = state.selected_target();
        assert_eq!(
            state.handle_input(mouse(MouseEventKind::Down(MouseButton::Left), column, row))?,
            InputOutcome::Ignored
        );
        assert_eq!(state.selected_target(), selected);
        assert_eq!(
            state.handle_input(mouse(MouseEventKind::ScrollDown, column, row))?,
            InputOutcome::Consumed
        );
        assert_ne!(state.selected_target(), selected);
        state.handle_input(mouse(MouseEventKind::ScrollUp, column, row))?;
        assert_eq!(state.selected_target(), selected);
    }
    Ok(())
}

#[test]
fn wheel_uses_only_current_rendered_panes() -> Result<(), Box<dyn Error>> {
    let mut state = MarkdownReviewState::new(document());
    let wheel = mouse(MouseEventKind::ScrollDown, 60, 2);
    assert_eq!(state.handle_input(wheel.clone())?, InputOutcome::Ignored);
    draw(&mut state, 100, 12);
    for (column, row) in [(0, 2), (99, 2), (60, 0), (60, 11), (100, 2)] {
        assert_eq!(
            state.handle_input(mouse(MouseEventKind::ScrollDown, column, row))?,
            InputOutcome::Ignored
        );
    }
    state.set_options(state.options().clone());
    assert_eq!(state.handle_input(wheel.clone())?, InputOutcome::Ignored);
    draw(&mut state, 100, 12);
    draw(&mut state, 40, 12);
    assert_eq!(state.handle_input(wheel.clone())?, InputOutcome::Ignored);
    draw(&mut state, 100, 12);
    let area = Rect::new(0, 0, 1, 1);
    MarkdownReviewWidget::new().render(area, &mut Buffer::empty(area), &mut state);
    assert_eq!(state.handle_input(wheel)?, InputOutcome::Ignored);
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

#[test]
fn changing_the_theme_reuses_retained_code_captures() -> Result<(), Box<dyn Error>> {
    let mut state = MarkdownReviewState::new(document());
    draw(&mut state, 100, 30);
    let before = state.highlight_stats();
    assert!(before.bytes > 0);
    state.set_theme(ReviewTheme::ayu()?);
    draw(&mut state, 100, 30);
    let after = state.highlight_stats();
    assert_eq!(after.bytes, before.bytes);
    assert_eq!(after.misses, before.misses);
    assert!(after.hits > before.hits);
    Ok(())
}

fn document() -> Arc<MarkdownDocument> {
    Arc::new(MarkdownDocument::parse_with_metadata(
        Some("plan.md".to_owned()),
        Some("Plan".to_owned()),
        "# Plan\n\nA **paragraph** with `code` and [a link](https://example.com).\n\n- first\n  - nested\n\n> quoted text\n\n| Name | Value |\n| --- | --- |\n| one | two |\n\n```rust\nlet value = 1;\nprintln!(\"{value}\");\n```\n",
    ))
}
