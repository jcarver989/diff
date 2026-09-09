use super::{MarkdownFocusPane, MarkdownReviewEvent, MarkdownReviewState};
use crate::{
    InputOutcome, InteractionPhase, ReviewInput,
    interaction::{self, ReviewWidget},
    theme_picker::ThemePicker,
};
use crate::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use clankerdiff_markdown::{MarkdownCommentDraft, MarkdownReviewError};
use clankerdiff_theme::ReviewTheme;
#[cfg(feature = "crossterm-backend")]
use crossterm::event::Event;
use ratatui::layout::Position;

/// Converts one Crossterm event and applies it to Markdown review state.
///
/// Key releases and unrelated terminal events are ignored. Submissions can
/// fail when a draft/comment body is blank, so validation is returned to the
/// host instead of being silently discarded.
///
/// # Errors
///
/// Returns [`MarkdownReviewError::BlankComment`] when an approval or
/// request-changes action encounters a blank comment body.
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

    /// Applies one input event.
    ///
    /// # Errors
    ///
    /// Returns [`MarkdownReviewError::BlankComment`] when an approval or
    /// request-changes action encounters a blank comment body.
    pub fn handle_input(
        &mut self,
        input: ReviewInput,
    ) -> Result<InputOutcome<MarkdownReviewEvent>, MarkdownReviewError> {
        interaction::handle_input(self, input)
    }
}

impl ReviewWidget for MarkdownReviewState {
    type Event = MarkdownReviewEvent;
    type Error = MarkdownReviewError;
    type Draft = MarkdownCommentDraft;

    fn phase(&self) -> InteractionPhase {
        self.interaction_phase()
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

    fn cancel_draft(&mut self) {
        self.session.cancel_draft();
    }

    fn submit_draft(&mut self) {
        self.session.submit_draft();
    }

    fn draft_changed(&mut self, _closed: bool) {
        self.request_follow();
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

    fn cancel_event() -> MarkdownReviewEvent {
        MarkdownReviewEvent::Cancel
    }

    fn handle_browse_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<InputOutcome<MarkdownReviewEvent>, MarkdownReviewError> {
        self.handle_browse_key(key)
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> InputOutcome<MarkdownReviewEvent> {
        self.handle_mouse(mouse);
        InputOutcome::Consumed
    }
}

impl MarkdownReviewState {
    fn handle_browse_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<InputOutcome<MarkdownReviewEvent>, MarkdownReviewError> {
        match key.code {
            KeyCode::Tab => {
                self.focus = match self.focus {
                    MarkdownFocusPane::Document => MarkdownFocusPane::Outline,
                    MarkdownFocusPane::Outline => MarkdownFocusPane::Document,
                }
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_target(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_target(1),
            KeyCode::Home | KeyCode::Char('g') => {
                self.session.select_first_target();
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.session.select_last_target();
            }
            KeyCode::Char('n') => {
                self.session.next_heading();
            }
            KeyCode::Char('p') => {
                self.session.previous_heading();
            }
            KeyCode::Left | KeyCode::Char('h') => self.focus = MarkdownFocusPane::Outline,
            KeyCode::Right | KeyCode::Char('l') => self.focus = MarkdownFocusPane::Document,
            KeyCode::Enter => {
                if self.focus == MarkdownFocusPane::Outline {
                    if let Some(target) = self.selected_outline_target() {
                        self.session.select_target(target);
                    }
                    self.focus = MarkdownFocusPane::Document;
                }
            }
            KeyCode::Char('c') => {
                self.session.begin_draft(None);
                self.request_follow();
            }
            KeyCode::Char('e') => {
                if self.session.comment_id_at_selection().is_some() {
                    self.session.edit_comment_at_selection();
                    self.request_follow();
                }
            }
            KeyCode::Char('x') => {
                self.session.delete_comment_at_selection();
                self.request_follow();
            }
            KeyCode::Char('u') => {
                self.session.undo_last_comment();
                self.request_follow();
            }
            KeyCode::Char('a') => return self.session.approve().map(InputOutcome::Emitted),
            KeyCode::Char('r') => return self.session.request_changes().map(InputOutcome::Emitted),
            KeyCode::Char('t') => {
                self.theme_picker = Some(ThemePicker::new(&self.theme));
            }
            KeyCode::Char('?') => self.help = true,
            _ => return Ok(InputOutcome::Ignored),
        }
        self.sync_outline_selection();
        self.request_follow();
        Ok(InputOutcome::Consumed)
    }

    fn move_target(&mut self, delta: isize) {
        self.session.move_target(delta);
        self.sync_outline_selection();
    }

    fn sync_outline_selection(&mut self) {
        if let Some(selected) = self.selected_target()
            && let Some(index) = self
                .document()
                .outline()
                .iter()
                .position(|heading| heading.target_id == selected)
        {
            self.outline_selected = index;
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        let position = Position::new(mouse.column, mouse.row);
        match mouse.kind {
            MouseEventKind::ScrollUp => self.move_target_with_follow(-1),
            MouseEventKind::ScrollDown => self.move_target_with_follow(1),
            MouseEventKind::Down(_) => {
                if let Some(region) = self
                    .hit_regions
                    .iter()
                    .rev()
                    .find(|region| region.area.contains(position))
                    .copied()
                {
                    if region.outline {
                        self.focus = MarkdownFocusPane::Outline;
                        if let Some(index) = self
                            .document()
                            .outline()
                            .iter()
                            .position(|heading| Some(heading.target_id) == region.target)
                        {
                            self.outline_selected = index;
                            let target = self.document().outline()[index].target_id;
                            self.session.select_target(target);
                        }
                    } else if let Some(target) = region.target {
                        self.focus = MarkdownFocusPane::Document;
                        self.session.select_target(target);
                    }
                    self.request_follow();
                }
            }
            _ => {}
        }
    }

    fn move_target_with_follow(&mut self, delta: isize) {
        let previous = self.selected_target();
        self.move_target(delta);
        if self.selected_target() != previous {
            self.request_follow();
        }
    }
}
