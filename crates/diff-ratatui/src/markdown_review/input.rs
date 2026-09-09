use super::{MarkdownFocusPane, MarkdownReviewEvent, MarkdownReviewState};
use crate::{
    InputOutcome, InteractionPhase, KeyBinding, KeyEvent, MarkdownReviewCommand, MouseEvent,
    MouseEventKind, NavigationPane, ReviewCommand, ReviewInput,
    interaction::{self, ReviewWidget},
    keybindings,
    theme_picker::ThemePicker,
};
use clankerdiff_core::{CommandContext, ReviewCapabilities};
use clankerdiff_markdown::{MarkdownCommentDraft, MarkdownReviewError};
#[cfg(feature = "crossterm-backend")]
use crossterm::event::Event;
use ratatui::layout::Position;
use std::sync::Arc;

#[cfg(feature = "crossterm-backend")]
pub fn handle_crossterm_event(
    state: &mut MarkdownReviewState,
    event: Event,
) -> Result<InputOutcome<MarkdownReviewEvent>, MarkdownReviewError> {
    crate::crossterm_adapter::handle_event(state, event)
}

impl MarkdownReviewState {
    #[must_use]
    pub fn interaction_phase(&self) -> InteractionPhase {
        if self.theme_picker.is_some() {
            InteractionPhase::ThemePicker
        } else if self.help {
            InteractionPhase::Help
        } else if self.session.draft().is_some() {
            InteractionPhase::Draft
        } else {
            InteractionPhase::Browse
        }
    }

    pub fn handle_input(
        &mut self,
        input: ReviewInput,
    ) -> Result<InputOutcome<MarkdownReviewEvent>, MarkdownReviewError> {
        interaction::handle_input(self, input)
    }

    #[must_use]
    pub fn command_for_key(&self, key: KeyEvent) -> Option<MarkdownReviewCommand> {
        if self.interaction_phase() != InteractionPhase::Browse {
            return None;
        }
        keybindings::binding_for_key(
            &self.keybindings,
            key,
            self.focus == MarkdownFocusPane::Document,
            false,
        )
        .map(|binding| binding.command)
    }

    pub(crate) fn help_bindings(&self) -> impl Iterator<Item = &KeyBinding<MarkdownReviewCommand>> {
        let context = CommandContext {
            phase: InteractionPhase::Browse,
            ..self.command_context()
        };
        keybindings::help_bindings(
            &self.keybindings,
            context.navigation_available,
            move |command| command.enabled(&context),
        )
    }

    pub(crate) fn footer_hint(&self, width: usize) -> String {
        keybindings::footer_hint(
            &self.keybindings,
            self.focus == MarkdownFocusPane::Document,
            false,
            |command| self.command_enabled(command),
            &ReviewCommand::ShowHelp.into(),
            width,
        )
    }

    #[must_use]
    pub fn command_context(&self) -> CommandContext {
        CommandContext {
            phase: self.interaction_phase(),
            capabilities: self.capabilities,
            navigation_available: !matches!(
                self.options.navigation,
                NavigationPane::Hidden | NavigationPane::Width(0)
            ),
            themes_available: !self.theme_choices.is_empty(),
            ..CommandContext::default()
        }
    }

    pub fn set_capabilities(&mut self, capabilities: ReviewCapabilities) {
        self.capabilities = capabilities;
        self.mark_dirty();
    }

    #[must_use]
    pub fn command_enabled(&self, command: &MarkdownReviewCommand) -> bool {
        command.enabled(&self.command_context())
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Exhaustive command dispatch keeps routing in one place"
    )]
    pub fn handle_command(
        &mut self,
        command: impl Into<MarkdownReviewCommand>,
    ) -> Result<InputOutcome<MarkdownReviewEvent>, MarkdownReviewError> {
        use MarkdownReviewCommand as C;
        use ReviewCommand as R;
        let command = command.into();
        if !self.command_enabled(&command) {
            return Ok(InputOutcome::Ignored);
        }
        let previous = (self.focus, self.session.selected_target());
        match command {
            C::Focus(pane) => self.focus = pane,
            C::ToggleFocus => {
                self.focus = match self.focus {
                    MarkdownFocusPane::Document => MarkdownFocusPane::Outline,
                    MarkdownFocusPane::Outline => MarkdownFocusPane::Document,
                }
            }
            C::MoveSelection(delta) => self.move_selection(delta),
            C::Page(delta) => self
                .move_selection(delta.saturating_mul(
                    isize::try_from(self.last_height.max(1)).unwrap_or(isize::MAX),
                )),
            C::First | C::Last => {
                let last = command == C::Last;
                if self.focus == MarkdownFocusPane::Outline {
                    self.select_heading(if last {
                        self.document().outline().len().saturating_sub(1)
                    } else {
                        0
                    });
                } else {
                    self.session.select_boundary(last);
                }
            }
            C::OpenSelected => {
                if self.focus == MarkdownFocusPane::Outline {
                    self.select_heading(self.outline_selected);
                    self.focus = MarkdownFocusPane::Document;
                }
            }
            C::Scroll { pane, lines } => {
                match pane {
                    MarkdownFocusPane::Document => {
                        self.scroll = self.scroll.saturating_add_signed(lines);
                        self.follow_pending = false;
                    }
                    MarkdownFocusPane::Outline => {
                        self.outline_scroll = self
                            .outline_scroll
                            .saturating_add_signed(lines)
                            .min(self.document().outline().len().saturating_sub(1));
                    }
                }
                self.mark_dirty();
                return Ok(InputOutcome::Consumed);
            }
            C::SelectTarget(target) => {
                if !self.session.select_target(target) {
                    return Ok(InputOutcome::Ignored);
                }
            }
            C::SelectHeading(index) => {
                if !self.select_heading(index) {
                    return Ok(InputOutcome::Ignored);
                }
            }
            C::NextHeading => {
                self.session.next_heading();
            }
            C::PreviousHeading => {
                self.session.previous_heading();
            }
            C::Approve => return self.session.approve().map(InputOutcome::Emitted),
            C::RequestChanges => return self.session.request_changes().map(InputOutcome::Emitted),
            C::CopyReview(decision) => {
                return Ok(InputOutcome::Emitted(self.session.copy_formatted(decision)));
            }
            C::Review(R::BeginComment | R::EditComment) => {
                let started = if command == C::Review(R::BeginComment) {
                    self.session.begin_draft(None)
                } else {
                    self.session.edit_comment_at_selection()
                };
                if !started {
                    return Ok(InputOutcome::Ignored);
                }
                self.request_follow();
            }
            C::Review(R::DeleteComment | R::UndoComment) => {
                let changed = if command == C::Review(R::DeleteComment) {
                    self.session.delete_comment_at_selection()
                } else {
                    self.session.undo_last_comment()
                };
                if !changed {
                    return Ok(InputOutcome::Ignored);
                }
                self.request_follow();
            }
            C::Review(R::SubmitComment) => {
                self.session.submit_draft();
                self.cursor_position = None;
                self.request_follow();
            }
            C::Review(R::Cancel) => {
                match self.interaction_phase() {
                    InteractionPhase::Browse => {
                        return Ok(InputOutcome::Emitted(MarkdownReviewEvent::Cancel));
                    }
                    InteractionPhase::Draft => {
                        self.session.cancel_draft();
                        self.cursor_position = None;
                        self.request_follow();
                    }
                    InteractionPhase::Help => self.help = false,
                    InteractionPhase::ThemePicker => {
                        if let Some(picker) = self.theme_picker.take() {
                            self.apply_theme(picker.cancel());
                        }
                    }
                    InteractionPhase::RepositoryPrompt => return Ok(InputOutcome::Ignored),
                }
                self.mark_dirty();
            }
            C::Review(R::ShowHelp) => {
                self.help = true;
                self.help_scroll = 0;
                self.mark_dirty();
            }
            C::Review(R::ScrollHelp(lines)) => {
                self.help_scroll = self
                    .help_scroll
                    .saturating_add_signed(lines)
                    .min(self.help_bindings().count().saturating_sub(1));
                self.mark_dirty();
            }
            C::Review(R::OpenThemePicker) => {
                self.theme_picker = ThemePicker::new(&self.theme, Arc::clone(&self.theme_choices));
                self.mark_dirty();
            }
            C::Review(R::SelectTheme(index)) => {
                let Some(theme) = self
                    .theme_picker
                    .as_mut()
                    .and_then(|picker| picker.select(index))
                else {
                    return Ok(InputOutcome::Ignored);
                };
                self.apply_theme(theme);
            }
            C::Review(R::MoveTheme(delta)) => {
                if let Some(picker) = self.theme_picker.as_mut() {
                    let theme = picker.select_relative(delta);
                    self.apply_theme(theme);
                }
            }
            C::Review(R::CommitTheme) => {
                let Some(picker) = self.theme_picker.take() else {
                    return Ok(InputOutcome::Ignored);
                };
                let theme = picker.commit();
                let id = theme.id().clone();
                self.apply_theme(theme);
                return Ok(InputOutcome::ThemeSelected(id));
            }
        }
        if previous != (self.focus, self.session.selected_target()) {
            self.sync_outline_selection();
            self.request_follow();
        }
        Ok(InputOutcome::Consumed)
    }

    fn select_heading(&mut self, index: usize) -> bool {
        let Some(heading) = self.document().outline().get(index) else {
            return false;
        };
        let target = heading.target_id;
        self.outline_selected = index;
        self.session.select_target(target)
    }

    fn move_selection(&mut self, delta: isize) {
        if self.focus == MarkdownFocusPane::Outline {
            self.select_heading(
                self.outline_selected
                    .saturating_add_signed(delta)
                    .min(self.document().outline().len().saturating_sub(1)),
            );
        } else {
            self.session.move_target(delta);
        }
    }

    fn sync_outline_selection(&mut self) {
        if let Some(selected) = self.selected_target()
            && let Some(index) = self
                .document()
                .outline()
                .iter()
                .rposition(|heading| heading.target_id.index() <= selected.index())
        {
            self.outline_selected = index;
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> InputOutcome<MarkdownReviewEvent> {
        let position = Position::new(mouse.column, mouse.row);
        match mouse.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                self.session
                    .move_target(if mouse.kind == MouseEventKind::ScrollUp {
                        -1
                    } else {
                        1
                    });
            }
            MouseEventKind::Down(_) => {
                if let Some(region) = self
                    .hit_regions
                    .iter()
                    .rev()
                    .find(|region| region.area.contains(position))
                    .copied()
                {
                    self.focus = if region.outline {
                        MarkdownFocusPane::Outline
                    } else {
                        MarkdownFocusPane::Document
                    };
                    if let Some(target) = region.target {
                        self.session.select_target(target);
                    }
                }
            }
            _ => return InputOutcome::Ignored,
        }
        self.sync_outline_selection();
        self.request_follow();
        InputOutcome::Consumed
    }
}

impl ReviewWidget for MarkdownReviewState {
    type Event = MarkdownReviewEvent;
    type Error = MarkdownReviewError;
    type Draft = MarkdownCommentDraft;

    fn phase(&self) -> InteractionPhase {
        self.interaction_phase()
    }
    fn handle_review_command(
        &mut self,
        command: ReviewCommand,
    ) -> Result<InputOutcome<MarkdownReviewEvent>, MarkdownReviewError> {
        self.handle_command(command)
    }
    fn contains(&self, position: Position) -> bool {
        self.hit_regions
            .iter()
            .any(|region| region.area.contains(position))
    }
    fn mark_dirty(&mut self) {
        Self::mark_dirty(self);
    }
    fn draft_mut(&mut self) -> Option<&mut MarkdownCommentDraft> {
        self.session.draft_mut()
    }
    fn draft_changed(&mut self) {
        self.request_follow();
    }
    fn handle_browse_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<InputOutcome<MarkdownReviewEvent>, MarkdownReviewError> {
        match self.command_for_key(key) {
            Some(command) => self.handle_command(command),
            None => Ok(InputOutcome::Ignored),
        }
    }
    fn handle_mouse(&mut self, mouse: MouseEvent) -> InputOutcome<MarkdownReviewEvent> {
        self.handle_mouse(mouse)
    }
}
