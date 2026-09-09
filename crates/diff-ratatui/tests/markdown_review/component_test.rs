use clankerdiff_ratatui::{
    BindingScope, InputOutcome, InteractionPhase, Key, KeyBinding, KeyEvent, KeyModifiers,
    MarkdownDocument, MarkdownFocusPane, MarkdownReview, MarkdownReviewCommand,
    MarkdownReviewError, MarkdownReviewEvent, MarkdownReviewState, MarkdownReviewWidget,
    MouseButton, MouseEvent, MouseEventKind, NavigationPane, ReviewCapabilities, ReviewCommand,
    ReviewInput, ReviewOptions, ReviewTheme, ThemeChoice, default_markdown_keybindings,
};
use ratatui::{Terminal, backend::TestBackend, buffer::Cell, layout::Rect, widgets::Paragraph};
use std::{error::Error, sync::Arc};

#[test]
fn builder_accepts_source_owned_and_shared_documents() {
    let source = "# Embedded\n\nReview text";
    let theme = ReviewTheme::builder("host").build();
    let review = MarkdownReview::builder()
        .theme(theme.clone())
        .markdown(source)
        .build();
    assert_eq!(review.state().document().source(), source);
    assert_eq!(review.state().theme().id(), theme.id());
    assert_eq!(review.state().options(), &ReviewOptions::default());
    assert_eq!(review.state().keybindings(), default_markdown_keybindings());
    assert!(review.state().theme_choices().is_empty());

    let review = MarkdownReview::builder()
        .document(MarkdownDocument::parse(source))
        .build();
    assert_eq!(review.state().document().source(), source);
    let document = Arc::new(MarkdownDocument::parse(source));
    let review = MarkdownReview::builder()
        .document(Arc::clone(&document))
        .build();
    assert!(Arc::ptr_eq(review.state().document(), &document));
}

#[test]
fn embedded_render_matches_low_level_widget_without_touching_host_area()
-> Result<(), Box<dyn Error>> {
    let document = Arc::new(MarkdownDocument::parse("# Embedded\n\nReview text"));
    let mut review = MarkdownReview::builder()
        .document(Arc::clone(&document))
        .embedded()
        .build();
    let mut state = MarkdownReviewState::new(document);
    state.set_options(ReviewOptions {
        footer: false,
        navigation: NavigationPane::Hidden,
    });
    let mut actual = Terminal::new(TestBackend::new(100, 30))?;
    let mut expected = Terminal::new(TestBackend::new(100, 30))?;
    let area = Rect::new(10, 5, 80, 20);
    assert!(review.is_dirty());
    actual.draw(|frame| {
        frame.render_widget(Paragraph::new("Host header"), Rect::new(0, 0, 100, 1));
        review.render(frame, area);
    })?;
    expected.draw(|frame| {
        frame.render_widget(Paragraph::new("Host header"), Rect::new(0, 0, 100, 1));
        frame.render_stateful_widget(MarkdownReviewWidget::new().borders(false), area, &mut state);
    })?;
    assert_eq!(actual.backend().buffer(), expected.backend().buffer());
    assert!(!review.is_dirty());
    assert_eq!(review.state().options(), state.options());
    Ok(())
}

#[test]
fn embedded_preset_can_be_overridden() -> Result<(), Box<dyn Error>> {
    let mut review = MarkdownReview::builder()
        .markdown("# Embedded")
        .embedded()
        .title("Host review")
        .borders(true)
        .footer(true)
        .navigation(NavigationPane::Width(12))
        .build();
    assert_eq!(
        review.state().options(),
        &ReviewOptions {
            footer: true,
            navigation: NavigationPane::Width(12)
        }
    );
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| review.render(frame, frame.area()))?;
    assert!(screen_text(&terminal).contains("Host review"));
    assert!(screen_text(&terminal).contains("0 comments"));
    Ok(())
}

#[test]
fn bindings_are_additive_and_derive_help_labels() -> Result<(), Box<dyn Error>> {
    let mut review = MarkdownReview::builder()
        .markdown("# One\n\nText\n\n## Two")
        .bind(Key::ctrl(Key::Enter), MarkdownReviewCommand::Approve)
        .bind(Key::alt('c'), ReviewCommand::BeginComment)
        .bind('a', ReviewCommand::ShowHelp)
        .build();
    assert_eq!(
        review.state().keybindings().len(),
        default_markdown_keybindings().len() + 3
    );
    assert_eq!(
        review.state().command_for_key(Key::Down.into()),
        Some(MarkdownReviewCommand::MoveSelection(1))
    );
    assert_eq!(
        review.state().command_for_key('a'.into()),
        Some(ReviewCommand::ShowHelp.into())
    );
    assert_eq!(
        review.state().command_for_key(Key::ctrl(Key::Enter)),
        Some(MarkdownReviewCommand::Approve)
    );
    assert_eq!(
        review.state().command_for_key(Key::alt('c')),
        Some(ReviewCommand::BeginComment.into())
    );
    review.handle_input('a')?;
    review.handle_command(ReviewCommand::ScrollHelp(20))?;
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| review.render(frame, frame.area()))?;
    let text = screen_text(&terminal);
    assert!(text.contains("Ctrl-Enter approve"));
    assert!(text.contains("Alt-c comment"));
    Ok(())
}

#[test]
fn explicit_keymaps_replace_defaults_and_scoped_overrides_keep_other_contexts()
-> Result<(), Box<dyn Error>> {
    let mut review = MarkdownReview::builder()
        .markdown("# One\n\nText")
        .keybindings(Vec::new())
        .bind('x', MarkdownReviewCommand::Approve)
        .binding(
            KeyBinding::new(
                'x'.into(),
                MarkdownReviewCommand::RequestChanges,
                "host reject",
            )
            .with_scope(BindingScope::Navigation),
        )
        .build();
    assert_eq!(review.state().keybindings().len(), 2);
    assert_eq!(review.state().command_for_key('a'.into()), None);
    assert_eq!(
        review.state().command_for_key('x'.into()),
        Some(MarkdownReviewCommand::Approve)
    );
    review.handle_command(MarkdownReviewCommand::Focus(MarkdownFocusPane::Outline))?;
    assert_eq!(
        review.state().command_for_key('x'.into()),
        Some(MarkdownReviewCommand::RequestChanges)
    );
    review.handle_command(ReviewCommand::ShowHelp)?;
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| review.render(frame, frame.area()))?;
    assert!(screen_text(&terminal).contains("host reject"));
    Ok(())
}

#[test]
fn draft_input_updates_dirty_state_and_frame_cursor() -> Result<(), Box<dyn Error>> {
    let mut review = MarkdownReview::builder()
        .markdown("# One\n\nText")
        .embedded()
        .build();
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    let area = Rect::new(10, 5, 80, 20);
    terminal.draw(|frame| review.render(frame, area))?;
    assert!(!review.is_dirty());
    assert_eq!(review.handle_input('c')?, InputOutcome::Consumed);
    assert!(review.is_dirty());
    assert_eq!(review.state().interaction_phase(), InteractionPhase::Draft);
    review.handle_input(ReviewInput::Paste("Looks good\nSecond line".into()))?;
    terminal.draw(|frame| review.render(frame, area))?;
    let position = review
        .state()
        .cursor_position()
        .ok_or("missing draft cursor")?;
    assert!(area.contains(position));
    assert_eq!(terminal.get_cursor_position()?, position);
    assert!(!review.is_dirty());
    review.handle_input(Key::Left)?;
    terminal.draw(|frame| review.render(frame, area))?;
    assert_ne!(review.state().cursor_position(), Some(position));
    assert_eq!(
        Some(terminal.get_cursor_position()?),
        review.state().cursor_position()
    );
    review.handle_command(ReviewCommand::SubmitComment)?;
    terminal.draw(|frame| review.render(frame, area))?;
    assert_eq!(review.state().cursor_position(), None);
    assert_eq!(review.state().review().len(), 1);
    let outcome = review.handle_command(MarkdownReviewCommand::Approve)?;
    assert!(
        matches!(outcome, InputOutcome::Emitted(MarkdownReviewEvent::Submit(submission)) if submission.comments.iter().any(|comment| comment.body == "Looks good\nSecond line"))
    );
    Ok(())
}

#[test]
fn mouse_input_uses_the_last_rendered_area() -> Result<(), Box<dyn Error>> {
    let mut review = MarkdownReview::builder()
        .markdown("# One\n\nText")
        .embedded()
        .build();
    assert_eq!(review.handle_input(click(11, 5))?, InputOutcome::Ignored);
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| review.render(frame, Rect::new(10, 5, 80, 20)))?;
    assert_eq!(review.handle_input(click(0, 0))?, InputOutcome::Ignored);
    assert_eq!(review.handle_input(click(11, 5))?, InputOutcome::Consumed);
    terminal.draw(|frame| review.render(frame, Rect::new(0, 0, 8, 3)))?;
    assert_eq!(review.handle_input(click(11, 5))?, InputOutcome::Ignored);
    Ok(())
}

#[test]
fn themes_are_host_owned_unless_choices_are_enabled() -> Result<(), Box<dyn Error>> {
    let theme = ReviewTheme::builder("host").build();
    let mut review = MarkdownReview::builder()
        .markdown("# One")
        .theme(theme.clone())
        .build();
    assert_eq!(
        review.handle_command(ReviewCommand::OpenThemePicker)?,
        InputOutcome::Ignored
    );
    assert_eq!(review.state().theme().id(), theme.id());
    let mut review = MarkdownReview::builder()
        .markdown("# One")
        .theme_choices(vec![ThemeChoice::new("Host theme", theme.clone())])
        .build();
    review.handle_command(ReviewCommand::OpenThemePicker)?;
    assert_eq!(
        review.handle_command(ReviewCommand::CommitTheme)?,
        InputOutcome::ThemeSelected(theme.id().clone())
    );
    assert_eq!(review.state().theme().id(), theme.id());
    Ok(())
}

#[test]
fn mutable_access_and_document_or_theme_replacement_invalidate_rendering()
-> Result<(), Box<dyn Error>> {
    let mut review = MarkdownReview::builder().markdown("# One").build();
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| review.render(frame, frame.area()))?;
    let _ = review.state_mut().review_mut();
    assert!(review.is_dirty());
    terminal.draw(|frame| review.render(frame, frame.area()))?;
    let replacement = Arc::new(MarkdownDocument::parse("# Two"));
    review.set_document(Arc::clone(&replacement));
    assert!(review.is_dirty());
    assert!(Arc::ptr_eq(review.state().document(), &replacement));
    terminal.draw(|frame| review.render(frame, frame.area()))?;
    let theme = ReviewTheme::builder("replacement").build();
    review.set_theme(theme.clone());
    assert!(review.is_dirty());
    assert_eq!(review.state().theme().id(), theme.id());
    terminal.draw(|frame| review.render(frame, frame.area()))?;
    review.mark_dirty();
    assert!(review.is_dirty());
    Ok(())
}

#[test]
fn capabilities_and_submission_errors_are_preserved() -> Result<(), Box<dyn Error>> {
    let mut review = MarkdownReview::builder()
        .markdown("# One")
        .capabilities(ReviewCapabilities {
            submit: false,
            ..ReviewCapabilities::default()
        })
        .build();
    assert_eq!(
        review.handle_command(MarkdownReviewCommand::Approve)?,
        InputOutcome::Ignored
    );
    review
        .state_mut()
        .set_capabilities(ReviewCapabilities::default());
    let document = Arc::clone(review.state().document());
    let target = review.state().selected_target().ok_or("missing target")?;
    let id = review
        .state_mut()
        .review_mut()
        .add_comment_for_target(&document, target, " ")
        .ok_or("missing comment")?;
    assert_eq!(
        review.handle_command(MarkdownReviewCommand::Approve),
        Err(MarkdownReviewError::BlankComment { id })
    );
    assert_eq!(
        review.handle_input('r'),
        Err(MarkdownReviewError::BlankComment { id })
    );
    Ok(())
}

#[test]
fn empty_documents_and_tiny_areas_are_supported() -> Result<(), Box<dyn Error>> {
    for source in ["", "# One"] {
        for area in [
            Rect::new(0, 0, 0, 0),
            Rect::new(0, 0, 1, 1),
            Rect::new(0, 0, 20, 1),
        ] {
            let mut review = MarkdownReview::builder()
                .markdown(source)
                .embedded()
                .build();
            let mut terminal = Terminal::new(TestBackend::new(20, 10))?;
            terminal.draw(|frame| review.render(frame, area))?;
            assert!(!review.is_dirty());
            assert_eq!(review.state().cursor_position(), None);
            assert!(matches!(
                review.handle_input(Key::Esc)?,
                InputOutcome::Emitted(MarkdownReviewEvent::Cancel)
            ));
        }
    }
    Ok(())
}

#[test]
fn typed_keys_support_modifiers_and_portable_input() {
    assert_eq!(
        Key::ctrl(Key::Enter),
        KeyEvent::new(Key::Enter, KeyModifiers::CONTROL)
    );
    assert_eq!(
        Key::alt('c'),
        KeyEvent::new(Key::Char('c'), KeyModifiers::ALT)
    );
    assert_eq!(
        Key::shift('a'),
        KeyEvent::new(Key::Char('a'), KeyModifiers::SHIFT)
    );
    assert_eq!(
        ReviewInput::from('c'),
        ReviewInput::Key(KeyEvent::new(Key::Char('c'), KeyModifiers::NONE))
    );
}

#[cfg(feature = "crossterm-backend")]
#[test]
fn borrowed_backend_events_preserve_routing_resize_and_release_behavior()
-> Result<(), Box<dyn Error>> {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    let mut review = MarkdownReview::builder().markdown("# One").build();
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| review.render(frame, frame.area()))?;
    let event = Event::Key(KeyEvent::new_with_kind(
        KeyCode::Char('c'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    ));
    assert_eq!(
        review.handle_crossterm_event(&event)?,
        InputOutcome::Ignored
    );
    assert!(!review.is_dirty());
    let event = Event::Resize(80, 20);
    assert_eq!(
        review.handle_crossterm_event(&event)?,
        InputOutcome::Ignored
    );
    assert!(review.is_dirty());
    assert_eq!(event, Event::Resize(80, 20));
    let event = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
    assert_eq!(
        review.handle_crossterm_event(&event)?,
        InputOutcome::Consumed
    );
    let event = Event::Paste("Host paste".into());
    assert_eq!(
        review.handle_crossterm_event(&event)?,
        InputOutcome::Consumed
    );
    assert_eq!(event, Event::Paste("Host paste".into()));
    assert_eq!(
        review
            .state()
            .session()
            .draft()
            .ok_or("missing draft")?
            .body(),
        "Host paste"
    );
    Ok(())
}

fn screen_text(terminal: &Terminal<TestBackend>) -> String {
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(Cell::symbol)
        .collect()
}

fn click(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}
