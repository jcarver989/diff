use crate::{
    DiffReviewCommand, DiffReviewState, FocusPane, InputOutcome, InteractionPhase, KeyCode,
    KeyEvent, MouseEvent, MouseEventKind, ReviewCommand, ReviewInput,
    interaction::{self, ReviewWidget},
    state::{RepositoryOperationStatus, RepositoryPrompt},
};
use clankerdiff_core::{CommentDraft, DiffReviewEvent, RepositoryAction};
#[cfg(feature = "crossterm-backend")]
use crossterm::event::Event;
use ratatui::layout::Position;
use std::convert::Infallible;

#[cfg(feature = "crossterm-backend")]
#[must_use]
pub fn handle_crossterm_event(
    state: &mut DiffReviewState,
    event: Event,
) -> InputOutcome<DiffReviewEvent> {
    let Ok(outcome) = crate::crossterm_adapter::handle_event(state, event);
    outcome
}

impl DiffReviewState {
    #[must_use]
    pub fn interaction_phase(&self) -> InteractionPhase {
        if self.theme_picker.is_some() {
            InteractionPhase::ThemePicker
        } else if self.repository_prompt.is_some() {
            InteractionPhase::RepositoryPrompt
        } else if self.help {
            InteractionPhase::Help
        } else if self.session.draft().is_some() {
            InteractionPhase::Draft
        } else {
            InteractionPhase::Browse
        }
    }

    #[must_use]
    pub fn handle_input(&mut self, input: ReviewInput) -> InputOutcome<DiffReviewEvent> {
        let Ok(outcome) = interaction::handle_input(self, input);
        outcome
    }
}

impl ReviewWidget for DiffReviewState {
    type Event = DiffReviewEvent;
    type Error = Infallible;
    type Draft = CommentDraft;

    fn phase(&self) -> InteractionPhase {
        self.interaction_phase()
    }
    fn handle_review_command(
        &mut self,
        command: ReviewCommand,
    ) -> Result<InputOutcome<DiffReviewEvent>, Infallible> {
        Ok(self.handle_command(command))
    }
    fn contains(&self, position: Position) -> bool {
        self.hit_layout.drawer.contains(position) || self.hit_layout.patch.contains(position)
    }
    fn mark_dirty(&mut self) {
        Self::mark_dirty(self);
    }
    fn draft_mut(&mut self) -> Option<&mut CommentDraft> {
        self.session.draft_mut()
    }
    fn draft_changed(&mut self) {
        self.request_follow();
    }
    fn paste_prompt(&mut self, text: &str) {
        if let Some(RepositoryPrompt::Commit { message }) = &mut self.repository_prompt {
            message.push_str(text);
        }
    }
    fn handle_browse_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<InputOutcome<DiffReviewEvent>, Infallible> {
        Ok(self
            .command_for_key(key)
            .map_or(InputOutcome::Ignored, |command| {
                self.handle_command(command)
            }))
    }
    fn handle_mouse(&mut self, mouse: MouseEvent) -> InputOutcome<DiffReviewEvent> {
        self.handle_mouse(mouse)
    }
    fn handle_prompt_key(&mut self, key: KeyEvent) -> InputOutcome<DiffReviewEvent> {
        self.handle_repository_prompt_key(key)
    }
}

impl DiffReviewState {
    fn handle_repository_prompt_key(&mut self, key: KeyEvent) -> InputOutcome<DiffReviewEvent> {
        if key.code == KeyCode::Esc {
            return self.handle_command(ReviewCommand::Cancel);
        }
        if !interaction::is_plain_key(key) {
            return InputOutcome::Consumed;
        }
        let Some(prompt) = self.repository_prompt.as_mut() else {
            return InputOutcome::Consumed;
        };
        let action = match prompt {
            RepositoryPrompt::Commit { message } => match key.code {
                KeyCode::Enter => {
                    let message = message.trim().to_owned();
                    if message.is_empty() {
                        self.set_repository_error("Commit message cannot be empty");
                        None
                    } else {
                        Some(RepositoryAction::Commit { message })
                    }
                }
                KeyCode::Backspace => {
                    message.pop();
                    None
                }
                KeyCode::Char(character) => {
                    message.push(character);
                    None
                }
                _ => None,
            },
            RepositoryPrompt::Discard { path, status } => match key.code {
                KeyCode::Char('y' | 'Y') => Some(RepositoryAction::Discard {
                    path: path.clone(),
                    status: *status,
                }),
                KeyCode::Char('n' | 'N') => {
                    self.repository_prompt = None;
                    None
                }
                _ => None,
            },
        };
        if let Some(action) = action {
            self.repository_prompt = None;
            return self.handle_command(DiffReviewCommand::RepositoryAction(action));
        }
        InputOutcome::Consumed
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> InputOutcome<DiffReviewEvent> {
        let position = Position::new(mouse.column, mouse.row);
        match mouse.kind {
            MouseEventKind::ScrollUp => self.scroll_at(position, -1),
            MouseEventKind::ScrollDown => self.scroll_at(position, 1),
            MouseEventKind::Down(_) => {
                if self.hit_layout.drawer.contains(position) {
                    self.focus = FocusPane::Files;
                    let relative = usize::from(mouse.row.saturating_sub(self.hit_layout.drawer.y));
                    self.select_drawer_entry(self.drawer_scroll.saturating_add(relative));
                    if self.hit_layout.drawer_stage_column == Some(mouse.column)
                        && !matches!(self.repository_status, RepositoryOperationStatus::Pending)
                    {
                        return self.handle_command(DiffReviewCommand::ToggleStage);
                    }
                } else if self.hit_layout.patch.contains(position) {
                    self.focus = FocusPane::Diff;
                    self.select_clicked_row(mouse.row);
                }
            }
            _ => {}
        }
        InputOutcome::Consumed
    }

    fn scroll_at(&mut self, position: Position, direction: isize) {
        let pane = if self.hit_layout.patch.contains(position) {
            FocusPane::Diff
        } else if self.hit_layout.drawer.contains(position) {
            FocusPane::Files
        } else {
            self.focus
        };
        match pane {
            FocusPane::Diff => self.move_row(direction),
            FocusPane::Files => self.scroll_drawer(direction),
        }
    }
}
