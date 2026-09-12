use clankerdiff_core::{DiffReviewEvent, ViewMode, testing::DocumentBuilder};
use clankerdiff_markdown::MarkdownDocument;
use clankerdiff_ratatui::{
    DiffReviewCommand, DiffReviewState, DiffReviewWidget, InputOutcome, InteractionPhase, KeyCode,
    KeyEvent, KeyModifiers, MarkdownReviewState, MouseEvent, MouseEventKind, NavigationPane,
    ReviewInput, ReviewOptions, ThemeChoice,
};
use ratatui::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};
use std::{error::Error, fmt::Write, sync::Arc};

#[test]
fn portable_review_routes_input_without_a_terminal() -> Result<(), Box<dyn Error>> {
    let mut state = DiffReviewState::new(
        DocumentBuilder::new()
            .changed("a.rs", "old\n", "new\n")
            .build(),
    );
    assert!(matches!(
        state.handle_input(key(KeyCode::F(1))),
        InputOutcome::Ignored
    ));
    assert_eq!(state.interaction_phase(), InteractionPhase::Browse);
    let _ = state.handle_input(key(KeyCode::Tab));
    let _ = state.handle_input(key(KeyCode::Char('c')));
    assert_eq!(state.interaction_phase(), InteractionPhase::Draft);
    let _ = state.handle_input(ReviewInput::Paste("héllo".to_owned()));
    let _ = state.handle_input(key(KeyCode::Left));
    let _ = state.handle_input(key(KeyCode::Backspace));
    let _ = state.handle_input(key(KeyCode::Char('p')));
    assert_eq!(state.session().draft().ok_or("no draft")?.body(), "hélpo");
    let _ = state.handle_input(key(KeyCode::Enter));
    assert_eq!(state.review().len(), 1);
    assert!(matches!(
        state.handle_input(key(KeyCode::Esc)),
        InputOutcome::Emitted(DiffReviewEvent::Cancel)
    ));
    Ok(())
}

#[test]
fn modals_consume_keys_and_outside_mouse_is_ignored() {
    let mut state =
        DiffReviewState::new(DocumentBuilder::new().changed("a.rs", "old", "new").build());
    let area = Rect::new(10, 5, 60, 15);
    let mut buffer = Buffer::empty(Rect::new(0, 0, 90, 30));
    for cell in &mut buffer.content {
        cell.set_symbol("~");
    }
    DiffReviewWidget::new().render(area, &mut buffer, &mut state);
    for y in 0..30 {
        for x in 0..90 {
            if !area.contains((x, y).into()) {
                assert_eq!(buffer[(x, y)].symbol(), "~");
            }
        }
    }
    assert!(matches!(
        state.handle_input(ReviewInput::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE
        })),
        InputOutcome::Ignored
    ));
    let _ = state.handle_input(key(KeyCode::Char('?')));
    assert_eq!(state.interaction_phase(), InteractionPhase::Help);
    assert!(matches!(
        state.handle_input(key(KeyCode::F(1))),
        InputOutcome::Consumed
    ));
    let _ = state.handle_input(key(KeyCode::Esc));
    state.set_theme_choices(vec![ThemeChoice::new("Current", state.theme().clone())]);
    let _ = state.handle_input(key(KeyCode::Char('t')));
    assert_eq!(state.interaction_phase(), InteractionPhase::ThemePicker);
    let _ = state.handle_input(key(KeyCode::Esc));
    assert_eq!(state.interaction_phase(), InteractionPhase::Browse);
}

#[test]
fn markdown_drafts_use_portable_input() -> Result<(), Box<dyn Error>> {
    let mut state = MarkdownReviewState::new(Arc::new(MarkdownDocument::parse("# Plan\n\nText")));
    let _ = state.handle_input(key(KeyCode::Char('c')))?;
    let _ = state.handle_input(ReviewInput::Paste("A comment".to_owned()))?;
    assert_eq!(state.interaction_phase(), InteractionPhase::Draft);
    let _ = state.handle_input(key(KeyCode::Enter))?;
    assert_eq!(state.review().len(), 1);
    assert!(matches!(
        state.handle_input(key(KeyCode::F(1)))?,
        InputOutcome::Ignored
    ));
    Ok(())
}

#[test]
fn deep_scrolling_preserves_source_content_and_logical_selection() -> Result<(), Box<dyn Error>> {
    let mut source = String::new();
    for index in 0..100 {
        write!(source, "{index:04}_")?;
    }
    let document = DocumentBuilder::new()
        .changed(
            "long.rs",
            "context\nshort\n",
            &format!("context\n{source}\n"),
        )
        .build();
    let make_state = || {
        let mut state = DiffReviewState::new(document.clone());
        state.set_options(ReviewOptions {
            navigation: NavigationPane::Hidden,
            footer: false,
            ..Default::default()
        });
        state.set_view_mode(ViewMode::Split);
        state
    };
    let mut state = make_state();
    let mut reference_state = make_state();
    let tall_area = Rect::new(0, 0, 40, 100);
    let mut reference = Buffer::empty(tall_area);
    DiffReviewWidget::new()
        .borders(false)
        .render(tall_area, &mut reference, &mut reference_state);
    let area = Rect::new(0, 0, 40, 6);
    let mut buffer = Buffer::empty(area);
    DiffReviewWidget::new()
        .borders(false)
        .render(area, &mut buffer, &mut state);
    let selected = state.selected_row();
    for _ in 0..3 {
        let _ = state.handle_input(key(KeyCode::PageDown));
        buffer.reset();
        DiffReviewWidget::new()
            .borders(false)
            .render(area, &mut buffer, &mut state);
    }
    let offset = u16::try_from(state.scroll_offset())?;
    assert!(offset > area.height);
    assert_eq!(state.selected_row(), selected);
    for y in 0..area.height {
        for x in 26..39 {
            assert_eq!(
                buffer[(x, y)].symbol(),
                reference[(x, y + offset)].symbol(),
                "at {x},{y}"
            );
        }
    }
    let expected = state
        .presentation()
        .rows(0..state.presentation().row_count())
        .iter()
        .position(|row| row.cells().any(|cell| cell.text.as_ref() == source))
        .ok_or("missing logical row")?;
    let _ = state.handle_input(ReviewInput::Mouse(MouseEvent {
        kind: MouseEventKind::Down(clankerdiff_ratatui::MouseButton::Left),
        column: 2,
        row: 2,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(state.selected_row(), Some(expected));
    assert_eq!(state.scroll_offset(), usize::from(offset));
    Ok(())
}

#[test]
fn viewport_starting_inside_a_comment_keeps_source_hit_mapping() -> Result<(), Box<dyn Error>> {
    let mut state = DiffReviewState::new(
        DocumentBuilder::new()
            .changed("note.rs", "", "source\nnext\n")
            .build(),
    );
    state.set_options(ReviewOptions {
        navigation: NavigationPane::Hidden,
        footer: false,
        ..Default::default()
    });
    let source = state.selected_row().ok_or("missing selected source")?;
    let anchor = state.session().selected_anchor().ok_or("missing anchor")?;
    let mut comment = String::new();
    for line in 0..20 {
        writeln!(comment, "note {line}")?;
    }
    state.review_mut().add_comment(anchor, comment);
    state.handle_command(DiffReviewCommand::SelectRow(source + 1));
    let area = Rect::new(0, 0, 30, 6);
    let mut buffer = Buffer::empty(area);
    DiffReviewWidget::new()
        .borders(false)
        .render(area, &mut buffer, &mut state);
    let _ = state.handle_input(key(KeyCode::PageUp));
    buffer.reset();
    DiffReviewWidget::new()
        .borders(false)
        .render(area, &mut buffer, &mut state);
    let top: String = (0..area.width).map(|x| buffer[(x, 0)].symbol()).collect();
    assert!(top.contains("note"), "{top}");
    let _ = state.handle_input(ReviewInput::Mouse(MouseEvent {
        kind: MouseEventKind::Down(clankerdiff_ratatui::MouseButton::Left),
        column: 3,
        row: 0,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(state.selected_row(), Some(source));
    Ok(())
}

#[cfg(feature = "crossterm-backend")]
#[test]
fn crossterm_adapter_matches_portable_input_and_ignores_releases() {
    use clankerdiff_ratatui::handle_crossterm_event;
    use crossterm::event::{
        Event, KeyCode as TerminalCode, KeyEvent as TerminalKey, KeyEventKind,
        KeyModifiers as TerminalModifiers,
    };
    let document = DocumentBuilder::new().changed("a.rs", "old", "new").build();
    let mut portable = DiffReviewState::new(document.clone());
    let mut terminal = DiffReviewState::new(document);
    for (code, terminal_code) in [
        (KeyCode::Tab, TerminalCode::Tab),
        (KeyCode::Down, TerminalCode::Down),
        (KeyCode::Char('c'), TerminalCode::Char('c')),
    ] {
        assert_eq!(
            portable.handle_input(key(code)).is_consumed(),
            handle_crossterm_event(
                &mut terminal,
                Event::Key(TerminalKey::new(terminal_code, TerminalModifiers::NONE))
            )
            .is_consumed()
        );
        assert_eq!(portable.selected_row(), terminal.selected_row());
        assert_eq!(portable.interaction_phase(), terminal.interaction_phase());
    }
    let release = TerminalKey::new_with_kind(
        TerminalCode::Esc,
        TerminalModifiers::NONE,
        KeyEventKind::Release,
    );
    assert!(matches!(
        handle_crossterm_event(&mut terminal, Event::Key(release)),
        InputOutcome::Ignored
    ));
    assert_eq!(terminal.interaction_phase(), InteractionPhase::Draft);
}

fn key(code: KeyCode) -> ReviewInput {
    ReviewInput::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
