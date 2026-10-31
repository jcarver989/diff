use crate::{
    DiffReviewCommand, DiffReviewState, FocusPane, InputOutcome, InteractionPhase, KeyCode, KeyEvent, KeyModifiers,
    MouseButton, MouseEvent, MouseEventKind, ReviewCommand, ReviewInput,
    definition_card::links_held,
    interaction::{self, ReviewWidget},
    render::FILE_CHEVRON_WIDTH,
    state::{Overlay, RepositoryOperationStatus, RepositoryPrompt},
};
use clankerdiff_core::{CommentDraft, DiffReviewEvent, RepositoryAction};
#[cfg(feature = "crossterm-backend")]
use crossterm::event::Event;
use ratatui::layout::{Position, Rect};
use std::convert::Infallible;

#[cfg(feature = "crossterm-backend")]
#[must_use]
pub fn handle_crossterm_event(state: &mut DiffReviewState, event: Event) -> InputOutcome<DiffReviewEvent> {
    let Ok(outcome) = crate::crossterm_adapter::handle_event(state, event);
    outcome
}

impl DiffReviewState {
    #[must_use]
    pub fn interaction_phase(&self) -> InteractionPhase {
        match &self.overlay {
            Some(overlay) => overlay.phase(),
            None if self.session.draft().is_some() => InteractionPhase::Draft,
            None => InteractionPhase::Browse,
        }
    }

    #[must_use]
    pub fn handle_input(&mut self, input: ReviewInput) -> InputOutcome<DiffReviewEvent> {
        let Ok(outcome) = interaction::handle_input(self, input);
        self.install_deferred();
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
    fn handle_review_command(&mut self, command: ReviewCommand) -> Result<InputOutcome<DiffReviewEvent>, Infallible> {
        Ok(self.handle_command(command))
    }
    fn contains(&self, position: Position) -> bool {
        self.hit_layout.drawer.contains(position)
            || self.hit_layout.patch.contains(position)
            || self.hit_at(position).is_some()
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
        if let Some(Overlay::Repository(RepositoryPrompt::Commit { message })) = &mut self.overlay {
            message.push_str(text);
        }
    }
    fn handle_browse_key(&mut self, key: KeyEvent) -> Result<InputOutcome<DiffReviewEvent>, Infallible> {
        if self.set_links_visible(false) {
            self.mark_dirty();
        }
        Ok(self.command_for_key(key).map_or(InputOutcome::Ignored, |command| self.handle_command(command)))
    }
    fn handle_mouse(&mut self, mouse: MouseEvent) -> InputOutcome<DiffReviewEvent> {
        self.handle_mouse(mouse)
    }
    fn handle_draft_mouse(&mut self, mouse: MouseEvent) -> InputOutcome<DiffReviewEvent> {
        if !matches!(mouse.kind, MouseEventKind::Down(_))
            || !self.hit_layout.patch.contains(Position::new(mouse.column, mouse.row))
            || !self.session.draft().is_some_and(|draft| draft.body().is_empty())
        {
            return InputOutcome::Consumed;
        }
        let clicked = self.patch_hits.row_at(mouse.row);
        if let Some(index) = clicked
            && self.session.presentation().gap_info(index).is_none()
            && self.session.select_row(index)
            && self.session.begin_draft(None)
        {
            self.focus = FocusPane::Diff;
            self.request_follow();
            self.mark_dirty();
        }
        InputOutcome::Consumed
    }
    fn handle_prompt_key(&mut self, key: KeyEvent) -> InputOutcome<DiffReviewEvent> {
        self.handle_repository_prompt_key(key)
    }
    fn handle_picker_key(&mut self, key: KeyEvent) -> InputOutcome<DiffReviewEvent> {
        self.handle_commit_picker_key(key)
    }
    fn handle_help_key(&mut self, key: KeyEvent) -> Option<InputOutcome<DiffReviewEvent>> {
        Some(self.handle_palette_key(key))
    }
    fn handle_overlay_mouse(&mut self, mouse: MouseEvent) -> InputOutcome<DiffReviewEvent> {
        let position = Position::new(mouse.column, mouse.row);
        let phase = self.interaction_phase();
        let command = match mouse.kind {
            MouseEventKind::Moved => {
                if self.track_pointer(position) {
                    self.mark_dirty();
                }
                return InputOutcome::Consumed;
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let step = if mouse.kind == MouseEventKind::ScrollUp { -1 } else { 1 };
                match phase {
                    InteractionPhase::Help => ReviewCommand::ScrollHelp(step).into(),
                    InteractionPhase::CommitPicker => DiffReviewCommand::MoveCommitPicker(step),
                    _ => return InputOutcome::Consumed,
                }
            }
            MouseEventKind::Down(MouseButton::Left) => match self.hit_at(position).map(|hit| hit.command.clone()) {
                Some(command) if phase == InteractionPhase::Help => return self.run_palette_entry(command),
                Some(command) => command,
                None if matches!(phase, InteractionPhase::Help | InteractionPhase::CommitPicker)
                    && self.hit_layout.overlay.is_some_and(|overlay| !overlay.contains(position)) =>
                {
                    ReviewCommand::Cancel.into()
                }
                None => return InputOutcome::Consumed,
            },
            _ => return InputOutcome::Consumed,
        };
        self.handle_command(command).or_consumed()
    }
    fn handle_modifiers(&mut self, modifiers: KeyModifiers) -> InputOutcome<DiffReviewEvent> {
        if self.interaction_phase() != InteractionPhase::Browse || !self.set_links_visible(links_held(modifiers)) {
            return InputOutcome::Ignored;
        }
        if let Some(pointer) = self.pointer {
            self.hover(pointer);
        }
        self.mark_dirty();
        InputOutcome::Consumed
    }
}

impl DiffReviewState {
    fn handle_commit_picker_key(&mut self, key: KeyEvent) -> InputOutcome<DiffReviewEvent> {
        if !interaction::is_plain_key(key) {
            return InputOutcome::Consumed;
        }
        let highlighted = match &self.overlay {
            Some(Overlay::Commits(picker)) => picker.highlighted(),
            _ => 0,
        };
        let command = match key.code {
            KeyCode::Esc | KeyCode::Char('q' | 'p') => ReviewCommand::Cancel.into(),
            KeyCode::Enter => DiffReviewCommand::GoToPage(highlighted),
            KeyCode::Up | KeyCode::Char('k') => DiffReviewCommand::MoveCommitPicker(-1),
            KeyCode::Down | KeyCode::Char('j') => DiffReviewCommand::MoveCommitPicker(1),
            KeyCode::Home | KeyCode::Char('g') => DiffReviewCommand::MoveCommitPicker(isize::MIN),
            KeyCode::End | KeyCode::Char('G') => DiffReviewCommand::MoveCommitPicker(isize::MAX),
            KeyCode::Char(digit @ '1'..='9') => {
                DiffReviewCommand::GoToPage(digit.to_digit(10).map_or(0, |digit| digit as usize - 1))
            }
            _ => return InputOutcome::Consumed,
        };
        self.handle_command(command).or_consumed()
    }

    fn handle_repository_prompt_key(&mut self, key: KeyEvent) -> InputOutcome<DiffReviewEvent> {
        if key.code == KeyCode::Esc {
            return self.handle_command(ReviewCommand::Cancel);
        }
        if !interaction::is_plain_key(key) {
            return InputOutcome::Consumed;
        }
        let Some(Overlay::Repository(prompt)) = self.overlay.as_mut() else {
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
                KeyCode::Char('y' | 'Y') => Some(RepositoryAction::Discard { path: path.clone(), status: *status }),
                KeyCode::Char('n' | 'N') => {
                    self.overlay = None;
                    None
                }
                _ => None,
            },
        };
        if let Some(action) = action {
            self.overlay = None;
            return self.handle_command(DiffReviewCommand::RepositoryAction(action));
        }
        InputOutcome::Consumed
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> InputOutcome<DiffReviewEvent> {
        let position = Position::new(mouse.column, mouse.row);
        let links_changed = self.set_links_visible(links_held(mouse.modifiers));
        if mouse.kind == MouseEventKind::Moved {
            let pointer_changed = self.track_pointer(position);
            let card_changed = self.hover(position);
            return if card_changed || links_changed || pointer_changed {
                InputOutcome::Consumed
            } else {
                InputOutcome::Ignored
            };
        }
        let left = mouse.kind == MouseEventKind::Down(MouseButton::Left);
        if left
            && self.links_visible
            && let Some(outcome) = self.follow_link(position)
        {
            return outcome;
        }
        if matches!(mouse.kind, MouseEventKind::Down(_)) && self.over_card(position) {
            return InputOutcome::Consumed;
        }
        self.names.close();
        if left && let Some(command) = self.hit_at(position).map(|hit| hit.command.clone()) {
            return self.handle_command(command).or_consumed();
        }
        match mouse.kind {
            MouseEventKind::ScrollUp => self.scroll_at(position, -1),
            MouseEventKind::ScrollDown => self.scroll_at(position, 1),
            MouseEventKind::Down(_) if self.hit_layout.drawer.contains(position) => return self.click_drawer(position),
            MouseEventKind::Down(_) if self.hit_layout.patch.contains(position) => {
                self.focus = FocusPane::Diff;
                if self.comment_at(position) {
                    return self.handle_command(ReviewCommand::BeginComment);
                }
                self.select_clicked_row(position);
            }
            _ => {}
        }
        InputOutcome::Consumed
    }

    fn click_drawer(&mut self, position: Position) -> InputOutcome<DiffReviewEvent> {
        self.focus = FocusPane::Files;
        let relative = usize::from(position.y.saturating_sub(self.hit_layout.drawer.y));
        self.select_drawer_entry(self.drawer_scroll.saturating_add(relative));
        if self.hit_layout.drawer_stage_column == Some(position.x) {
            if matches!(self.repository_status, RepositoryOperationStatus::Pending) {
                return InputOutcome::Consumed;
            }
            return self.handle_command(DiffReviewCommand::ToggleStage);
        }
        match self.drawer.selected().filter(|entry| entry.directory).map(|entry| entry.expanded) {
            Some(true) => self.collapse_drawer_entry(),
            Some(false) => {
                self.expand_or_open_drawer_entry();
            }
            None => {}
        }
        InputOutcome::Consumed
    }

    pub(crate) fn track_pointer(&mut self, position: Position) -> bool {
        let before = self.hover_target(self.pointer);
        self.pointer = Some(position);
        before != self.hover_target(self.pointer)
    }

    fn hover_target(&self, pointer: Option<Position>) -> Option<[Option<Rect>; 4]> {
        let pointer = pointer?;
        let patch = self.hit_layout.patch;
        let panes = [self.hit_layout.drawer, patch].into_iter();
        let row =
            panes.filter(|pane| pane.contains(pointer)).map(|pane| Rect { y: pointer.y, height: 1, ..pane }).next();
        let cells = self.patch_hits.code_cells.iter().map(|cell| cell.area.union(cell.gutter));
        let chevron = row.filter(|_| patch.contains(pointer) && pointer.x < patch.x.saturating_add(FILE_CHEVRON_WIDTH));
        Some([
            self.hit_at(pointer).map(|hit| hit.area),
            cells.into_iter().find(|area| area.contains(pointer)),
            row,
            chevron,
        ])
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
            FocusPane::Diff => self.scroll_patch(direction),
            FocusPane::Files => self.scroll_drawer(direction),
        }
    }
}
