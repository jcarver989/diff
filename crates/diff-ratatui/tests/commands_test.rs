use clankerdiff_core::{
    DiffReviewEvent, DiffScope, RepositoryAction, RevealAmount, testing::DocumentBuilder,
};
use clankerdiff_markdown::{MarkdownDocument, MarkdownReviewEvent};
use clankerdiff_ratatui::{
    BindingScope, DiffReviewCommand as DiffCommand, DiffReviewState, DiffReviewWidget, FocusPane,
    InputOutcome, InteractionPhase, KeyBinding, KeyCode, KeyEvent, KeyModifiers, MarkdownFocusPane,
    MarkdownReviewCommand as MarkdownCommand, MarkdownReviewState, MarkdownReviewWidget,
    ReviewCommand, ReviewInput,
};
use clankerdiff_theme::ThemeChoice;
use ratatui::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};
use std::{error::Error, sync::Arc};

#[test]
fn replacement_bindings_and_direct_commands_are_independent() {
    let mut state = diff();
    let shortcut = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    state.set_keybindings(vec![KeyBinding::new(
        shortcut,
        DiffCommand::Review(ReviewCommand::BeginComment),
        "annotate",
    )]);
    assert!(matches!(
        state.handle_input(key(KeyCode::Tab)),
        InputOutcome::Ignored
    ));
    assert!(matches!(
        state.handle_input(key(KeyCode::Char('c'))),
        InputOutcome::Ignored
    ));
    assert_eq!(
        state.command_for_key(shortcut),
        Some(ReviewCommand::BeginComment.into())
    );
    assert!(matches!(
        state.handle_input(ReviewInput::Key(shortcut)),
        InputOutcome::Consumed
    ));
    assert_eq!(state.interaction_phase(), InteractionPhase::Draft);
    state.set_keybindings(Vec::new());
    state.handle_command(ReviewCommand::Cancel);
    assert!(matches!(
        state.handle_input(key(KeyCode::Esc)),
        InputOutcome::Ignored
    ));
    assert!(matches!(
        state.handle_command(ReviewCommand::Cancel),
        InputOutcome::Emitted(DiffReviewEvent::Cancel)
    ));
}

#[test]
fn later_bindings_override_in_their_context() {
    let mut state = diff();
    let shortcut = KeyEvent::new(KeyCode::F(2), KeyModifiers::ALT);
    state.set_keybindings(vec![
        KeyBinding::new(shortcut, DiffCommand::Refresh, "reload"),
        KeyBinding::new(shortcut, DiffCommand::CopyReview, "copy")
            .with_scope(BindingScope::Document),
    ]);
    assert_eq!(state.command_for_key(shortcut), Some(DiffCommand::Refresh));
    state.handle_command(DiffCommand::Focus(FocusPane::Diff));
    assert_eq!(
        state.command_for_key(shortcut),
        Some(DiffCommand::CopyReview)
    );
    assert_eq!(
        state.command_for_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE)),
        None
    );
}

#[test]
fn default_shortcuts_do_not_capture_composed_keys() -> Result<(), Box<dyn Error>> {
    let mut state = diff();
    state.handle_command(DiffCommand::Focus(FocusPane::Diff));
    let mut markdown = markdown();
    for modifiers in [
        KeyModifiers::CONTROL,
        KeyModifiers::ALT,
        KeyModifiers::SUPER,
    ] {
        let input = ReviewInput::Key(KeyEvent::new(KeyCode::Char('c'), modifiers));
        assert!(matches!(
            state.handle_input(input.clone()),
            InputOutcome::Ignored
        ));
        assert!(matches!(
            markdown.handle_input(input)?,
            InputOutcome::Ignored
        ));
    }
    assert_eq!(
        state.command_for_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT)),
        Some(DiffCommand::CycleScope)
    );
    Ok(())
}

#[test]
fn default_keyboard_and_semantic_commands_produce_the_same_review() {
    let mut keyboard = diff();
    let mut commands = diff();
    for (code, command) in [
        (KeyCode::Tab, DiffCommand::ToggleFocus),
        (KeyCode::Down, DiffCommand::MoveSelection(1)),
        (KeyCode::Char('c'), ReviewCommand::BeginComment.into()),
    ] {
        let _ = keyboard.handle_input(key(code));
        commands.handle_command(command);
    }
    for state in [&mut keyboard, &mut commands] {
        let _ = state.handle_input(ReviewInput::Paste("Keep this comment".into()));
    }
    let _ = keyboard.handle_input(key(KeyCode::Enter));
    commands.handle_command(ReviewCommand::SubmitComment);
    assert_eq!(keyboard.review(), commands.review());
    assert_eq!(keyboard.selected_row(), commands.selected_row());
    assert_eq!(commands.review().len(), 1);
    for command in [
        DiffCommand::StageAll,
        DiffCommand::Refresh,
        DiffCommand::SetScope(DiffScope::Staged),
    ] {
        assert!(matches!(
            commands.handle_command(command),
            InputOutcome::Emitted(_)
        ));
        assert!(!commands.repository_pending());
        assert_eq!(commands.scope(), DiffScope::Both);
        assert_eq!(commands.review().len(), 1);
    }
    assert!(matches!(
        commands.handle_command(DiffCommand::RepositoryAction(RepositoryAction::UnstageAll)),
        InputOutcome::Emitted(DiffReviewEvent::RepositoryAction(
            RepositoryAction::UnstageAll
        ))
    ));
    commands.set_repository_pending();
    assert!(matches!(
        commands.handle_command(DiffCommand::StageAll),
        InputOutcome::Ignored
    ));
    assert_eq!(commands.review().len(), 1);
    assert!(matches!(
        commands.handle_command(ReviewCommand::Cancel),
        InputOutcome::Emitted(DiffReviewEvent::Cancel)
    ));
}

#[test]
fn direct_file_selection_synchronizes_navigation() {
    let mut state = DiffReviewState::new(
        DocumentBuilder::new()
            .changed("a.rs", "old", "new")
            .changed("nested/b.rs", "old", "new")
            .build(),
    );
    let area = Rect::new(0, 0, 100, 20);
    DiffReviewWidget::new().render(area, &mut Buffer::empty(area), &mut state);
    assert!(!state.is_dirty());
    assert!(matches!(
        state.handle_command(DiffCommand::SelectFile(1)),
        InputOutcome::Consumed
    ));
    assert!(state.is_dirty());
    assert_eq!(state.selected_file(), Some(1));
    state.handle_command(DiffCommand::OpenSelected);
    assert_eq!(state.focus(), FocusPane::Diff);
    assert_eq!(state.selected_file(), Some(1));
    assert!(matches!(
        state.handle_command(DiffCommand::SelectFile(100)),
        InputOutcome::Ignored
    ));
    assert!(matches!(
        state.handle_command(DiffCommand::SelectRow(usize::MAX)),
        InputOutcome::Ignored
    ));
}

#[test]
fn markdown_bindings_and_outline_commands_work_without_default_keys() -> Result<(), Box<dyn Error>>
{
    let mut state = markdown();
    let shortcut = KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT);
    state.set_keybindings(vec![KeyBinding::new(
        shortcut,
        MarkdownCommand::Approve,
        "accept",
    )]);
    assert!(matches!(
        state.handle_input(key(KeyCode::Char('a')))?,
        InputOutcome::Ignored
    ));
    assert!(matches!(
        state.handle_input(ReviewInput::Key(shortcut))?,
        InputOutcome::Emitted(MarkdownReviewEvent::Submit(_))
    ));
    state.set_keybindings(Vec::new());
    state.handle_command(MarkdownCommand::Focus(MarkdownFocusPane::Outline))?;
    state.handle_command(MarkdownCommand::MoveSelection(1))?;
    let second_heading = state.document().outline()[1].target_id;
    assert_eq!(state.selected_target(), Some(second_heading));
    state.handle_command(MarkdownCommand::OpenSelected)?;
    assert_eq!(state.focus(), MarkdownFocusPane::Document);
    assert_eq!(state.selected_target(), Some(second_heading));
    state.handle_command(ReviewCommand::BeginComment)?;
    state.handle_input(ReviewInput::Paste("Review this".into()))?;
    state.handle_command(ReviewCommand::SubmitComment)?;
    assert_eq!(state.review().len(), 1);
    assert!(matches!(
        state.handle_command(ReviewCommand::Cancel)?,
        InputOutcome::Emitted(MarkdownReviewEvent::Cancel)
    ));
    assert_eq!(state.review().len(), 1);
    Ok(())
}

#[test]
fn repository_commands_depend_on_capabilities_not_footer_visibility() {
    let mut state = diff();
    state.set_options(clankerdiff_ratatui::ReviewOptions {
        footer: false,
        ..Default::default()
    });
    assert!(state.command_enabled(&DiffCommand::BeginCommit));
    assert!(matches!(
        state.handle_command(DiffCommand::BeginCommit),
        InputOutcome::Consumed
    ));
    assert_eq!(
        state.interaction_phase(),
        InteractionPhase::RepositoryPrompt
    );
    let area = Rect::new(0, 0, 100, 20);
    DiffReviewWidget::new().render(area, &mut Buffer::empty(area), &mut state);
    state.handle_command(ReviewCommand::Cancel);
    state.set_capabilities(clankerdiff_core::ReviewCapabilities {
        repository: false,
        ..Default::default()
    });
    assert!(!state.command_enabled(&DiffCommand::BeginCommit));
    assert!(matches!(
        state.handle_command(DiffCommand::StageAll),
        InputOutcome::Ignored
    ));
}

#[test]
fn no_op_commands_and_keys_leave_rendered_states_clean() -> Result<(), Box<dyn Error>> {
    let mut diff = diff();
    let mut markdown = markdown();
    let area = Rect::new(0, 0, 100, 20);
    DiffReviewWidget::new().render(area, &mut Buffer::empty(area), &mut diff);
    MarkdownReviewWidget::new().render(area, &mut Buffer::empty(area), &mut markdown);
    let row = diff.selected_row().ok_or("missing diff row")?;
    let target = markdown
        .selected_target()
        .ok_or("missing Markdown target")?;
    let diff_command = DiffCommand::SelectRow(row);
    let markdown_command = MarkdownCommand::SelectTarget(target);
    let shortcut = KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE);
    diff.set_keybindings(vec![KeyBinding::new(
        shortcut,
        diff_command.clone(),
        "select",
    )]);
    markdown.set_keybindings(vec![KeyBinding::new(shortcut, markdown_command, "select")]);
    DiffReviewWidget::new().render(area, &mut Buffer::empty(area), &mut diff);
    MarkdownReviewWidget::new().render(area, &mut Buffer::empty(area), &mut markdown);
    assert_eq!(diff.handle_command(diff_command), InputOutcome::Consumed);
    assert_eq!(
        markdown.handle_command(markdown_command)?,
        InputOutcome::Consumed
    );
    assert_eq!(
        diff.handle_input(ReviewInput::Key(shortcut)),
        InputOutcome::Consumed
    );
    assert_eq!(
        markdown.handle_input(ReviewInput::Key(shortcut))?,
        InputOutcome::Consumed
    );
    assert!(!diff.is_dirty());
    assert!(!markdown.is_dirty());
    diff.handle_command(ReviewCommand::BeginComment);
    markdown.handle_command(ReviewCommand::BeginComment)?;
    assert!(diff.is_dirty());
    assert!(markdown.is_dirty());
    DiffReviewWidget::new().render(area, &mut Buffer::empty(area), &mut diff);
    MarkdownReviewWidget::new().render(area, &mut Buffer::empty(area), &mut markdown);
    diff.handle_command(ReviewCommand::SubmitComment);
    markdown.handle_command(ReviewCommand::SubmitComment)?;
    assert!(diff.is_dirty());
    assert!(markdown.is_dirty());
    assert_eq!(diff.interaction_phase(), InteractionPhase::Browse);
    assert_eq!(markdown.interaction_phase(), InteractionPhase::Browse);
    Ok(())
}

#[test]
fn unchanged_layout_and_heading_commands_leave_rendered_states_clean() -> Result<(), Box<dyn Error>>
{
    let mut diff = diff();
    let mut markdown = markdown();
    markdown.handle_command(MarkdownCommand::SelectHeading(0))?;
    let area = Rect::new(0, 0, 100, 20);
    DiffReviewWidget::new().render(area, &mut Buffer::empty(area), &mut diff);
    MarkdownReviewWidget::new().render(area, &mut Buffer::empty(area), &mut markdown);
    for command in [
        DiffCommand::SelectSide(diff.selected_side()),
        DiffCommand::SetViewMode(diff.view_mode()),
        DiffCommand::RevealGap(RevealAmount::All),
    ] {
        assert_eq!(diff.handle_command(command), InputOutcome::Consumed);
        assert!(!diff.is_dirty());
    }
    for command in [
        MarkdownCommand::SelectHeading(0),
        MarkdownCommand::PreviousHeading,
        MarkdownCommand::First,
    ] {
        assert_eq!(markdown.handle_command(command)?, InputOutcome::Consumed);
        assert!(!markdown.is_dirty());
    }
    Ok(())
}

#[test]
fn loading_blocks_comment_mutations_but_not_host_refresh() {
    let mut state = diff();
    state.set_loading();
    for command in [
        ReviewCommand::BeginComment,
        ReviewCommand::EditComment,
        ReviewCommand::DeleteComment,
        ReviewCommand::UndoComment,
        ReviewCommand::SubmitComment,
    ] {
        assert_eq!(state.handle_command(command), InputOutcome::Ignored);
    }
    assert_eq!(
        state.handle_command(DiffCommand::Refresh),
        InputOutcome::Emitted(DiffReviewEvent::Refresh)
    );
    assert!(!state.command_context().document_ready);
    assert!(!state.repository_pending());
}

#[test]
fn modal_commands_block_browse_actions_and_cancel_locally() {
    let mut state = diff();
    state.set_theme_choices(ThemeChoice::catalog());
    for (command, phase) in [
        (ReviewCommand::BeginComment.into(), InteractionPhase::Draft),
        (ReviewCommand::ShowHelp.into(), InteractionPhase::Help),
        (
            ReviewCommand::OpenThemePicker.into(),
            InteractionPhase::ThemePicker,
        ),
        (DiffCommand::BeginCommit, InteractionPhase::RepositoryPrompt),
    ] {
        assert_eq!(state.handle_command(command), InputOutcome::Consumed);
        assert_eq!(state.interaction_phase(), phase);
        assert_eq!(
            state.handle_command(DiffCommand::Refresh),
            InputOutcome::Ignored
        );
        assert_eq!(
            state.handle_command(DiffCommand::SelectRow(0)),
            InputOutcome::Ignored
        );
        assert_eq!(
            state.handle_command(ReviewCommand::DeleteComment),
            InputOutcome::Ignored
        );
        assert_eq!(
            state.handle_command(ReviewCommand::Cancel),
            InputOutcome::Consumed
        );
        assert_eq!(state.interaction_phase(), InteractionPhase::Browse);
    }
    assert_eq!(
        state.handle_command(ReviewCommand::Cancel),
        InputOutcome::Emitted(DiffReviewEvent::Cancel)
    );
}

fn diff() -> DiffReviewState {
    DiffReviewState::new(
        DocumentBuilder::new()
            .changed("a.rs", "old\n", "new\n")
            .build(),
    )
}

fn markdown() -> MarkdownReviewState {
    MarkdownReviewState::new(Arc::new(MarkdownDocument::parse(
        "# One\n\nText\n\n## Two\n\nMore text",
    )))
}

fn key(code: KeyCode) -> ReviewInput {
    ReviewInput::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
