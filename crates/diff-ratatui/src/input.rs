//! Crossterm keyboard, mouse, and paste input helpers.

use crate::{DiffReviewEvent, DiffReviewState, FocusPane};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use diff_core::DiffSide;
use ratatui::layout::Position;

const WHEEL_ROWS: isize = 3;

/// Framework-neutral input accepted by [`DiffReviewState::handle_input`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffReviewInput {
    /// A Crossterm key event.
    Key(KeyEvent),
    /// Pasted text.
    Paste(String),
    /// A Crossterm mouse event.
    Mouse(MouseEvent),
}

/// Converts one Crossterm event and applies it to state.
///
/// Non-input events and key releases are ignored.
#[must_use]
pub fn handle_crossterm_event(
    state: &mut DiffReviewState,
    event: Event,
) -> Option<DiffReviewEvent> {
    match event {
        Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            state.handle_input(DiffReviewInput::Key(key))
        }
        Event::Paste(text) => state.handle_input(DiffReviewInput::Paste(text)),
        Event::Mouse(mouse) => state.handle_input(DiffReviewInput::Mouse(mouse)),
        _ => None,
    }
}

impl DiffReviewState {
    #[must_use]
    pub fn handle_input(&mut self, input: DiffReviewInput) -> Option<DiffReviewEvent> {
        match input {
            DiffReviewInput::Key(key) => self.handle_key(key),
            DiffReviewInput::Paste(text) => {
                if let Some(draft) = self.session.draft_mut() {
                    draft.insert(&text);
                }
                None
            }
            DiffReviewInput::Mouse(mouse) => {
                self.handle_mouse(mouse);
                None
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<DiffReviewEvent> {
        if self.help {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
                self.help = false;
            }
            return None;
        }
        if self.session.draft().is_some() {
            self.handle_draft_key(key);
            return None;
        }
        let cancels = key.code == KeyCode::Esc
            || (key.code == KeyCode::Char('g') && key.modifiers.contains(KeyModifiers::CONTROL));
        if cancels {
            return Some(DiffReviewEvent::Cancel);
        }
        if key
            .modifiers
            .intersects(KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::CONTROL)
        {
            return None;
        }
        self.handle_browse_key(key)
    }

    fn handle_browse_key(&mut self, key: KeyEvent) -> Option<DiffReviewEvent> {
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
            KeyCode::Left | KeyCode::Char('h') => self.focus = FocusPane::Files,
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => self.focus = FocusPane::Diff,
            KeyCode::Up | KeyCode::Char('k') => self.move_focused(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_focused(1),
            KeyCode::PageUp => self.page(-1),
            KeyCode::PageDown => self.page(1),
            KeyCode::Home => self.select_boundary(false),
            KeyCode::End => self.select_boundary(true),
            KeyCode::Char('c') if in_diff => {
                self.session.begin_draft(None);
            }
            KeyCode::Char('e') if in_diff => {
                let editing = self.session.comment_id_at_selection();
                if editing.is_some() {
                    self.session.begin_draft(editing);
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
                return Some(DiffReviewEvent::SubmitReview(self.session.submission()));
            }
            KeyCode::Char('y') if in_diff => {
                return Some(DiffReviewEvent::CopyFormattedReview(
                    self.session.submission().formatted,
                ));
            }
            KeyCode::Char('v') => {
                if self.session.cycle_view_mode() {
                    self.scroll_to_selected_file();
                }
            }
            KeyCode::Char('?') => self.help = true,
            _ => {}
        }
        None
    }

    fn move_focused(&mut self, delta: isize) {
        match self.focus {
            FocusPane::Files => self.move_file(delta),
            FocusPane::Diff => self.move_row(delta),
        }
    }

    fn handle_draft_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.session.cancel_draft();
            self.cursor_position = None;
            return;
        }
        if key.code == KeyCode::Enter && !key.modifiers.contains(KeyModifiers::SHIFT) {
            self.session.submit_draft();
            self.cursor_position = None;
            return;
        }
        let Some(draft) = self.session.draft_mut() else {
            return;
        };
        match key.code {
            KeyCode::Enter => draft.insert("\n"),
            KeyCode::Left => draft.move_cursor_left(),
            KeyCode::Right => draft.move_cursor_right(),
            KeyCode::Home => draft.move_cursor_to_start(),
            KeyCode::End => draft.move_cursor_to_end(),
            KeyCode::Backspace => draft.delete_before_cursor(),
            KeyCode::Delete => draft.delete_at_cursor(),
            KeyCode::Char(character)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                let mut buffer = [0_u8; 4];
                draft.insert(character.encode_utf8(&mut buffer));
            }
            _ => {}
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.move_focused(-WHEEL_ROWS),
            MouseEventKind::ScrollDown => self.move_focused(WHEEL_ROWS),
            MouseEventKind::Down(_) => {
                let position = Position::new(mouse.column, mouse.row);
                if self.hit_layout.drawer.contains(position) {
                    self.focus = FocusPane::Files;
                    let relative = usize::from(mouse.row.saturating_sub(self.hit_layout.drawer.y));
                    self.select_file(self.drawer_scroll.saturating_add(relative));
                } else if self.hit_layout.patch.contains(position) {
                    self.focus = FocusPane::Diff;
                    self.select_clicked_row(mouse.row);
                }
            }
            _ => {}
        }
    }
}
