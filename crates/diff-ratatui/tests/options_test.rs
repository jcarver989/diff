use clankerdiff_core::{ReviewCapabilities, testing::DocumentBuilder};
use clankerdiff_markdown::MarkdownDocument;
use clankerdiff_ratatui::{
    BindingScope, DiffReviewCommand, DiffReviewState, DiffReviewWidget, FocusPane, InputOutcome,
    KeyBinding, KeyCode, KeyEvent, KeyModifiers, MarkdownFocusPane, MarkdownReviewCommand,
    MarkdownReviewState, MarkdownReviewWidget, MouseButton, MouseEvent, MouseEventKind,
    NavigationPane, ReviewCommand, ReviewInput, ReviewOptions,
};
use ratatui::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};
use std::{error::Error, sync::Arc};

#[test]
fn hidden_chrome_uses_the_full_area_and_clears_hit_regions() -> Result<(), Box<dyn Error>> {
    let mut diff = diff();
    let mut markdown = markdown();
    let options = ReviewOptions {
        footer: false,
        navigation: NavigationPane::Hidden,
    };
    diff.set_options(options.clone());
    markdown.set_options(options);
    assert_eq!(diff.focus(), FocusPane::Diff);
    assert_eq!(markdown.focus(), MarkdownFocusPane::Document);
    assert!(matches!(
        diff.handle_command(DiffReviewCommand::ToggleFocus),
        InputOutcome::Ignored
    ));
    assert!(matches!(
        markdown.handle_command(MarkdownReviewCommand::ToggleFocus)?,
        InputOutcome::Ignored
    ));
    for width in [0, 1, 2, 5, 40, 100] {
        for height in [0, 1, 2, 10] {
            let area = Rect::new(4, 3, width, height);
            let outer = Rect::new(0, 0, 110, 20);
            let mut buffer = filled(outer);
            DiffReviewWidget::new()
                .borders(false)
                .render(area, &mut buffer, &mut diff);
            assert_outside_untouched(&buffer, area);
            let mut buffer = filled(outer);
            MarkdownReviewWidget::new()
                .borders(false)
                .render(area, &mut buffer, &mut markdown);
            assert_outside_untouched(&buffer, area);
        }
    }
    let area = Rect::new(4, 3, 80, 10);
    DiffReviewWidget::new().borders(false).render(
        area,
        &mut Buffer::empty(Rect::new(0, 0, 110, 20)),
        &mut diff,
    );
    diff.set_loading();
    DiffReviewWidget::new().borders(false).render(
        area,
        &mut Buffer::empty(Rect::new(0, 0, 110, 20)),
        &mut diff,
    );
    assert!(matches!(
        diff.handle_input(click(10, 5)),
        InputOutcome::Ignored
    ));
    MarkdownReviewWidget::new().render(
        Rect::new(0, 0, 0, 0),
        &mut Buffer::empty(Rect::new(0, 0, 110, 20)),
        &mut markdown,
    );
    assert!(matches!(
        markdown.handle_input(click(10, 5))?,
        InputOutcome::Ignored
    ));
    Ok(())
}

#[test]
fn caller_widths_determine_navigation_hit_regions() -> Result<(), Box<dyn Error>> {
    let area = Rect::new(4, 3, 60, 10);
    let mut diff = diff();
    let mut markdown = markdown();
    let options = ReviewOptions {
        footer: false,
        navigation: NavigationPane::Width(12),
    };
    diff.set_options(options.clone());
    markdown.set_options(options);
    let mut buffer = Buffer::empty(Rect::new(0, 0, 70, 20));
    DiffReviewWidget::new()
        .borders(false)
        .render(area, &mut buffer, &mut diff);
    let _ = diff.handle_input(click(6, 3));
    assert_eq!(diff.focus(), FocusPane::Files);
    let _ = diff.handle_input(click(20, 3));
    assert_eq!(diff.focus(), FocusPane::Diff);
    MarkdownReviewWidget::new()
        .borders(false)
        .render(area, &mut buffer, &mut markdown);
    markdown.handle_input(click(6, 3))?;
    assert_eq!(markdown.focus(), MarkdownFocusPane::Outline);
    markdown.handle_input(click(20, 3))?;
    assert_eq!(markdown.focus(), MarkdownFocusPane::Document);
    Ok(())
}

#[test]
fn help_and_footer_use_caller_bindings() -> Result<(), Box<dyn Error>> {
    let mut diff = diff();
    let mut markdown = markdown();
    let shortcut = KeyEvent::new(KeyCode::F(5), KeyModifiers::CONTROL);
    diff.set_keybindings(vec![KeyBinding::new(
        shortcut,
        DiffReviewCommand::Refresh,
        "host reload",
    )]);
    markdown.set_keybindings(vec![KeyBinding::new(
        shortcut,
        MarkdownReviewCommand::Approve,
        "host approve",
    )]);
    let area = Rect::new(0, 0, 100, 30);
    let mut buffer = Buffer::empty(area);
    DiffReviewWidget::new().render(area, &mut buffer, &mut diff);
    assert!(text(&buffer).contains("Ctrl-F(5) host reload"));
    diff.handle_command(ReviewCommand::ShowHelp);
    DiffReviewWidget::new().render(area, &mut buffer, &mut diff);
    assert!(text(&buffer).contains("host reload"));
    assert!(!text(&buffer).contains("stage/unstage"));
    markdown.handle_command(ReviewCommand::ShowHelp)?;
    MarkdownReviewWidget::new().render(area, &mut buffer, &mut markdown);
    assert!(text(&buffer).contains("host approve"));
    assert!(!text(&buffer).contains("request changes"));
    Ok(())
}

#[test]
fn help_filters_capabilities_and_resolves_overrides_in_browse_context() -> Result<(), Box<dyn Error>>
{
    let mut diff = diff();
    let mut markdown = markdown();
    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
    diff.set_keybindings(vec![
        KeyBinding::new(
            key(KeyCode::Char('a')),
            DiffReviewCommand::StageAll,
            "unsupported git",
        ),
        KeyBinding::new(
            key(KeyCode::Char('r')),
            DiffReviewCommand::Refresh,
            "unsupported refresh",
        ),
        KeyBinding::new(
            key(KeyCode::Char('S')),
            DiffReviewCommand::CycleScope,
            "unsupported scope",
        ),
        KeyBinding::new(
            key(KeyCode::Char('c')),
            ReviewCommand::BeginComment.into(),
            "replaced label",
        ),
        KeyBinding::new(
            key(KeyCode::Char('c')),
            ReviewCommand::BeginComment.into(),
            "current label",
        ),
        KeyBinding::new(
            key(KeyCode::Char('s')),
            DiffReviewCommand::SubmitReview,
            "submit supported",
        ),
    ]);
    markdown.set_keybindings(vec![
        KeyBinding::new(
            key(KeyCode::Char('a')),
            MarkdownReviewCommand::Approve,
            "unsupported approve",
        ),
        KeyBinding::new(
            key(KeyCode::Char('c')),
            ReviewCommand::BeginComment.into(),
            "replaced label",
        ),
        KeyBinding::new(
            key(KeyCode::Char('c')),
            ReviewCommand::BeginComment.into(),
            "current label",
        ),
    ]);
    diff.set_capabilities(ReviewCapabilities {
        repository: false,
        refresh: false,
        scope: false,
        ..Default::default()
    });
    markdown.set_capabilities(ReviewCapabilities {
        submit: false,
        ..Default::default()
    });
    diff.handle_command(ReviewCommand::ShowHelp);
    markdown.handle_command(ReviewCommand::ShowHelp)?;
    let area = Rect::new(0, 0, 100, 30);
    let mut buffer = Buffer::empty(area);
    DiffReviewWidget::new().render(area, &mut buffer, &mut diff);
    let rendered = text(&buffer);
    assert!(rendered.contains("current label") && rendered.contains("submit supported"));
    assert!(!rendered.contains("unsupported") && !rendered.contains("replaced label"));
    let mut buffer = Buffer::empty(area);
    MarkdownReviewWidget::new().render(area, &mut buffer, &mut markdown);
    let rendered = text(&buffer);
    assert!(rendered.contains("current label"));
    assert!(!rendered.contains("unsupported") && !rendered.contains("replaced label"));
    Ok(())
}

#[test]
fn hidden_navigation_removes_unreachable_bindings_from_help() -> Result<(), Box<dyn Error>> {
    let mut diff = diff();
    let mut markdown = markdown();
    let shortcut = KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE);
    diff.set_keybindings(vec![
        KeyBinding::new(
            shortcut,
            DiffReviewCommand::MoveSelection(1),
            "document command",
        ),
        KeyBinding::new(
            shortcut,
            DiffReviewCommand::MoveSelection(-1),
            "navigation command",
        )
        .with_scope(BindingScope::Navigation),
    ]);
    markdown.set_keybindings(vec![
        KeyBinding::new(
            shortcut,
            MarkdownReviewCommand::MoveSelection(1),
            "document command",
        ),
        KeyBinding::new(
            shortcut,
            MarkdownReviewCommand::MoveSelection(-1),
            "navigation command",
        )
        .with_scope(BindingScope::Navigation),
    ]);
    let options = ReviewOptions {
        navigation: NavigationPane::Hidden,
        ..Default::default()
    };
    diff.set_options(options.clone());
    markdown.set_options(options);
    diff.handle_command(ReviewCommand::ShowHelp);
    markdown.handle_command(ReviewCommand::ShowHelp)?;
    let area = Rect::new(0, 0, 100, 30);
    let mut buffer = Buffer::empty(area);
    DiffReviewWidget::new().render(area, &mut buffer, &mut diff);
    assert!(text(&buffer).contains("document command"));
    assert!(!text(&buffer).contains("navigation command"));
    let mut buffer = Buffer::empty(area);
    MarkdownReviewWidget::new().render(area, &mut buffer, &mut markdown);
    assert!(text(&buffer).contains("document command"));
    assert!(!text(&buffer).contains("navigation command"));
    Ok(())
}

#[test]
fn footers_deduplicate_aliases_and_keep_help_and_status_visible() {
    let mut diff = diff();
    let mut markdown = markdown();
    for width in [30, 40, 80, 120] {
        let area = Rect::new(0, 0, width, 20);
        let mut buffer = Buffer::empty(area);
        DiffReviewWidget::new()
            .borders(false)
            .render(area, &mut buffer, &mut diff);
        let hint = footer(&buffer);
        assert!(
            hint.contains("? help") && hint.contains("0 comments"),
            "{hint}"
        );
        assert!(hint.matches("previous").count() <= 1);
        assert!(hint.matches("next").count() <= 1);
        let mut buffer = Buffer::empty(area);
        MarkdownReviewWidget::new()
            .borders(false)
            .render(area, &mut buffer, &mut markdown);
        let hint = footer(&buffer);
        assert!(
            hint.contains("? help") && hint.contains("0 comments"),
            "{hint}"
        );
        assert!(hint.matches("previous").count() <= 1);
        assert!(hint.matches("next").count() <= 1);
    }
}

fn footer(buffer: &Buffer) -> String {
    let y = buffer.area.bottom() - 1;
    (buffer.area.x..buffer.area.right())
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn diff() -> DiffReviewState {
    DiffReviewState::new(DocumentBuilder::new().changed("a.rs", "old", "new").build())
}

fn markdown() -> MarkdownReviewState {
    MarkdownReviewState::new(Arc::new(MarkdownDocument::parse("# Plan\n\nText")))
}

fn click(column: u16, row: u16) -> ReviewInput {
    ReviewInput::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn filled(area: Rect) -> Buffer {
    let mut buffer = Buffer::empty(area);
    for cell in &mut buffer.content {
        cell.set_symbol("~");
    }
    buffer
}

fn assert_outside_untouched(buffer: &Buffer, area: Rect) {
    for y in buffer.area.y..buffer.area.bottom() {
        for x in buffer.area.x..buffer.area.right() {
            if !area.contains((x, y).into()) {
                assert_eq!(buffer[(x, y)].symbol(), "~");
            }
        }
    }
}

fn text(buffer: &Buffer) -> String {
    buffer.content.iter().map(|cell| cell.symbol()).collect()
}
