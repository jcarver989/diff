//! Crossterm keyboard, mouse, and paste input helpers.

use crate::{
    DiffReviewState, FocusPane, InputOutcome, InteractionPhase, ReviewInput,
    interaction::{self, ReviewWidget},
    state::{RepositoryOperationStatus, RepositoryPrompt},
    theme_picker::ThemePicker,
};
use crate::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use clankerdiff_core::{CommentDraft, DiffReviewEvent, DiffSide, RepositoryAction, RevealAmount};
use clankerdiff_theme::ReviewTheme;
#[cfg(feature = "crossterm-backend")]
use crossterm::event::Event;
use ratatui::layout::Position;
use std::convert::Infallible;

const DRAWER_WHEEL_ROWS: isize = 1;

/// Converts one Crossterm event and applies it to state.
///
/// Non-input events and key releases are ignored.
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

    fn contains(&self, position: Position) -> bool {
        self.hit_layout.drawer.contains(position) || self.hit_layout.patch.contains(position)
    }

    fn mark_dirty(&mut self) {
        Self::mark_dirty(self);
    }

    fn draft_mut(&mut self) -> Option<&mut CommentDraft> {
        self.session.draft_mut()
    }

    fn cancel_draft(&mut self) {
        self.session.cancel_draft();
    }

    fn submit_draft(&mut self) {
        self.session.submit_draft();
    }

    fn draft_changed(&mut self, closed: bool) {
        if closed {
            self.cursor_position = None;
        } else {
            self.request_follow();
        }
    }

    fn theme_picker(&mut self) -> &mut Option<ThemePicker> {
        &mut self.theme_picker
    }

    fn set_theme(&mut self, theme: ReviewTheme) {
        Self::set_theme(self, theme);
    }

    fn close_help(&mut self) {
        self.help = false;
    }

    fn cancel_event() -> DiffReviewEvent {
        DiffReviewEvent::Cancel
    }

    fn handle_browse_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<InputOutcome<DiffReviewEvent>, Infallible> {
        Ok(self.handle_browse_key(key))
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> InputOutcome<DiffReviewEvent> {
        self.handle_mouse(mouse)
    }

    fn handle_prompt_key(&mut self, key: KeyEvent) -> InputOutcome<DiffReviewEvent> {
        self.handle_repository_prompt_key(key)
    }
}

impl DiffReviewState {
    #[allow(clippy::too_many_lines)]
    fn handle_browse_key(&mut self, key: KeyEvent) -> InputOutcome<DiffReviewEvent> {
        if matches!(self.repository_status, RepositoryOperationStatus::Pending)
            && matches!(
                key.code,
                KeyCode::Char(' ' | 'a' | 'A' | 'C' | 'S' | 'd' | 'r')
            )
        {
            return InputOutcome::Consumed;
        }
        let in_diff = self.focus == FocusPane::Diff;
        match key.code {
            KeyCode::Tab => {
                self.focus = match self.focus {
                    FocusPane::Files => FocusPane::Diff,
                    FocusPane::Diff => FocusPane::Files,
                };
            }
            KeyCode::Left if in_diff && self.layout().is_split() => {
                self.session.set_selected_side(DiffSide::Old);
            }
            KeyCode::Right if in_diff && self.layout().is_split() => {
                self.session.set_selected_side(DiffSide::New);
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if in_diff {
                    self.focus = FocusPane::Files;
                } else {
                    self.collapse_drawer_entry();
                }
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter if !in_diff => {
                if !self.expand_or_open_drawer_entry() {
                    self.focus = FocusPane::Diff;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_focused(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_focused(1),
            KeyCode::PageUp => self.page(-1),
            KeyCode::PageDown => self.page(1),
            KeyCode::Home => self.select_boundary(false),
            KeyCode::End => self.select_boundary(true),
            KeyCode::Char('c') if in_diff => {
                self.session.begin_draft(None);
                self.request_follow();
            }
            KeyCode::Char('e') if in_diff => {
                let editing = self.session.comment_id_at_selection();
                if editing.is_some() {
                    self.session.begin_draft(editing);
                    self.request_follow();
                }
            }
            KeyCode::Char('x') if in_diff => {
                self.session.delete_comment_at_selection();
            }
            KeyCode::Char('u') if in_diff => {
                if let Some(id) = self.session.last_comment_id() {
                    self.session.review_mut().remove_comment(id);
                }
            }
            KeyCode::Char('s') if in_diff => {
                return InputOutcome::Emitted(DiffReviewEvent::SubmitReview(
                    self.session.submission(),
                ));
            }
            KeyCode::Char('y') if in_diff => {
                return InputOutcome::Emitted(DiffReviewEvent::CopyFormattedReview(
                    self.session.submission().formatted,
                ));
            }
            KeyCode::Char(' ') if self.focus == FocusPane::Files => {
                if let Some(action) = self.toggle_stage_action() {
                    return InputOutcome::Emitted(DiffReviewEvent::RepositoryAction(action));
                }
            }
            KeyCode::Char('a') if self.focus == FocusPane::Files => {
                return InputOutcome::Emitted(DiffReviewEvent::RepositoryAction(
                    RepositoryAction::StageAll,
                ));
            }
            KeyCode::Char('A') if self.focus == FocusPane::Files => {
                return InputOutcome::Emitted(DiffReviewEvent::RepositoryAction(
                    RepositoryAction::UnstageAll,
                ));
            }
            KeyCode::Char('C') => self.begin_commit(),
            KeyCode::Char('d') => self.begin_discard(),
            KeyCode::Char('t') => {
                self.theme_picker = Some(ThemePicker::new(&self.theme));
            }
            KeyCode::Enter
                if in_diff
                    && self
                        .session
                        .selected_row()
                        .is_some_and(|row| self.presentation().gap_info(row).is_some()) =>
            {
                self.reveal_selected_gap(RevealAmount::Step);
            }
            KeyCode::Char('o') if in_diff => {
                self.reveal_selected_gap(RevealAmount::Step);
            }
            KeyCode::Char('O') if in_diff => {
                self.reveal_selected_gap(RevealAmount::All);
            }
            KeyCode::Char('f') if in_diff => {
                self.toggle_full_file();
            }
            KeyCode::Char('v') => {
                if self.session.cycle_view_mode() {
                    self.scroll_to_selected_file();
                }
            }
            KeyCode::Char('S') => {
                return InputOutcome::Emitted(DiffReviewEvent::SetScope(self.scope.next()));
            }
            KeyCode::Char('?') => self.help = true,
            _ => return InputOutcome::Ignored,
        }
        InputOutcome::Consumed
    }

    fn move_focused(&mut self, delta: isize) {
        match self.focus {
            FocusPane::Files => self.move_drawer_entry(delta),
            FocusPane::Diff => self.move_row(delta),
        }
    }

    fn handle_repository_prompt_key(&mut self, key: KeyEvent) -> InputOutcome<DiffReviewEvent> {
        let Some(prompt) = self.repository_prompt.as_mut() else {
            return InputOutcome::Consumed;
        };
        match prompt {
            RepositoryPrompt::Commit { message } => match key.code {
                KeyCode::Esc => self.repository_prompt = None,
                KeyCode::Enter => {
                    let message = message.trim().to_owned();
                    if message.is_empty() {
                        self.set_repository_error("Commit message cannot be empty");
                    } else {
                        self.repository_prompt = None;
                        return InputOutcome::Emitted(DiffReviewEvent::RepositoryAction(
                            RepositoryAction::Commit { message },
                        ));
                    }
                }
                KeyCode::Backspace => {
                    message.pop();
                }
                KeyCode::Char(character)
                    if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
                {
                    message.push(character);
                }
                _ => {}
            },
            RepositoryPrompt::Discard { path, status } => match key.code {
                KeyCode::Char('y' | 'Y') => {
                    let action = RepositoryAction::Discard {
                        path: path.clone(),
                        status: *status,
                    };
                    self.repository_prompt = None;
                    return InputOutcome::Emitted(DiffReviewEvent::RepositoryAction(action));
                }
                KeyCode::Char('n' | 'N') | KeyCode::Esc => self.repository_prompt = None,
                _ => {}
            },
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
                    self.mark_dirty();
                    if self.hit_layout.drawer_stage_column == Some(mouse.column)
                        && !matches!(self.repository_status, RepositoryOperationStatus::Pending)
                        && let Some(action) = self.toggle_stage_action()
                    {
                        return InputOutcome::Emitted(DiffReviewEvent::RepositoryAction(action));
                    }
                } else if self.hit_layout.patch.contains(position) {
                    self.focus = FocusPane::Diff;
                    self.select_clicked_row(mouse.row);
                    self.mark_dirty();
                }
            }
            _ => {}
        }
        InputOutcome::Consumed
    }

    /// Navigates the pane under the pointer, falling back to the focused pane
    /// when the pointer is over neither. The wheel moves one diff selection or
    /// one drawer entry per notch without changing which pane has focus.
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
            FocusPane::Files => self.scroll_drawer(direction * DRAWER_WHEEL_ROWS),
        }
    }
}
