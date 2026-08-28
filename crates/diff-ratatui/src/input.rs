//! Crossterm keyboard, mouse, and paste input helpers.

use crate::{DiffReviewEvent, DiffReviewState, FocusPane};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use diff_core::ViewMode;
use ratatui::layout::Position;

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
pub fn handle_crossterm_event(state: &mut DiffReviewState, event: Event) -> Vec<DiffReviewEvent> {
    match event {
        Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            state.handle_input(DiffReviewInput::Key(key))
        }
        Event::Paste(text) => state.handle_input(DiffReviewInput::Paste(text)),
        Event::Mouse(mouse) => state.handle_input(DiffReviewInput::Mouse(mouse)),
        _ => Vec::new(),
    }
}

impl DiffReviewState {
    /// Applies keyboard, mouse, or paste input and returns host events.
    #[must_use]
    pub fn handle_input(&mut self, input: DiffReviewInput) -> Vec<DiffReviewEvent> {
        match input {
            DiffReviewInput::Key(key) => self.handle_key(key),
            DiffReviewInput::Paste(text) => {
                self.insert_text(&text);
                Vec::new()
            }
            DiffReviewInput::Mouse(mouse) => {
                self.handle_mouse(mouse);
                Vec::new()
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<DiffReviewEvent> {
        if self.help {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
                self.help = false;
            }
            return Vec::new();
        }
        if self.draft.is_some() {
            return self.handle_draft_key(key);
        }
        if key
            .modifiers
            .intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
            || key.modifiers.contains(KeyModifiers::CONTROL) && key.code != KeyCode::Char('g')
        {
            return Vec::new();
        }
        self.handle_browse_key(key)
    }

    fn handle_browse_key(&mut self, key: KeyEvent) -> Vec<DiffReviewEvent> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('g')
                if key.code == KeyCode::Esc || key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                vec![DiffReviewEvent::Cancel]
            }
            KeyCode::Tab => {
                self.focus = match self.focus {
                    FocusPane::Files => FocusPane::Diff,
                    FocusPane::Diff => FocusPane::Files,
                };
                Vec::new()
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.focus = FocusPane::Files;
                Vec::new()
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => {
                self.focus = FocusPane::Diff;
                Vec::new()
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_focused(-1);
                Vec::new()
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_focused(1);
                Vec::new()
            }
            KeyCode::PageUp => {
                self.page(-1);
                Vec::new()
            }
            KeyCode::PageDown => {
                self.page(1);
                Vec::new()
            }
            KeyCode::Home => {
                self.select_boundary(false);
                Vec::new()
            }
            KeyCode::End => {
                self.select_boundary(true);
                Vec::new()
            }
            KeyCode::Char('c') if self.focus == FocusPane::Diff => {
                self.begin_draft(None);
                Vec::new()
            }
            KeyCode::Char('e') if self.focus == FocusPane::Diff => {
                if let Some(id) = self.last_comment_for_selection() {
                    self.begin_draft(Some(id));
                }
                Vec::new()
            }
            KeyCode::Char('x') if self.focus == FocusPane::Diff => {
                if let Some(id) = self.last_comment_for_selection() {
                    self.review.remove_comment(id);
                }
                Vec::new()
            }
            KeyCode::Char('u') if self.focus == FocusPane::Diff => {
                if let Some(id) = self.review.comments().last().map(|comment| comment.id) {
                    self.review.remove_comment(id);
                }
                Vec::new()
            }
            KeyCode::Char('s') if self.focus == FocusPane::Diff => {
                vec![DiffReviewEvent::SubmitReview(self.review.submission())]
            }
            KeyCode::Char('y') if self.focus == FocusPane::Diff => {
                vec![DiffReviewEvent::CopyFormattedReview(
                    self.review.submission().formatted,
                )]
            }
            KeyCode::Char('v') => {
                let mode = match self.view_mode {
                    ViewMode::Auto => ViewMode::Unified,
                    ViewMode::Unified => ViewMode::Split,
                    ViewMode::Split => ViewMode::Auto,
                };
                self.set_view_mode(mode);
                Vec::new()
            }
            KeyCode::Char('?') => {
                self.help = true;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn move_focused(&mut self, delta: isize) {
        match self.focus {
            FocusPane::Files => self.move_file(delta),
            FocusPane::Diff => self.move_row(delta),
        }
    }

    fn handle_draft_key(&mut self, key: KeyEvent) -> Vec<DiffReviewEvent> {
        match key.code {
            KeyCode::Esc => {
                self.draft = None;
                self.cursor_position = None;
            }
            KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.submit_draft();
                self.cursor_position = None;
            }
            KeyCode::Enter => self.insert_text("\n"),
            KeyCode::Left => self.move_draft_cursor(false),
            KeyCode::Right => self.move_draft_cursor(true),
            KeyCode::Home => {
                if let Some(draft) = &mut self.draft {
                    draft.cursor = 0;
                }
            }
            KeyCode::End => {
                if let Some(draft) = &mut self.draft {
                    draft.cursor = draft.text.len();
                }
            }
            KeyCode::Backspace => self.delete_before_cursor(),
            KeyCode::Delete => self.delete_at_cursor(),
            KeyCode::Char(character)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.insert_text(&character.to_string());
            }
            _ => {}
        }
        Vec::new()
    }

    fn insert_text(&mut self, text: &str) {
        let Some(draft) = &mut self.draft else {
            return;
        };
        draft.text.insert_str(draft.cursor, text);
        draft.cursor += text.len();
    }

    fn move_draft_cursor(&mut self, forward: bool) {
        let Some(draft) = &mut self.draft else {
            return;
        };
        if forward {
            draft.cursor = draft.text[draft.cursor..]
                .char_indices()
                .nth(1)
                .map_or(draft.text.len(), |(offset, _)| draft.cursor + offset);
        } else {
            draft.cursor = draft.text[..draft.cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(offset, _)| offset);
        }
    }

    fn delete_before_cursor(&mut self) {
        let Some(draft) = &mut self.draft else {
            return;
        };
        let previous = draft.text[..draft.cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(offset, _)| offset);
        draft.text.replace_range(previous..draft.cursor, "");
        draft.cursor = previous;
    }

    fn delete_at_cursor(&mut self) {
        let Some(draft) = &mut self.draft else {
            return;
        };
        let next = draft.text[draft.cursor..]
            .char_indices()
            .nth(1)
            .map_or(draft.text.len(), |(offset, _)| draft.cursor + offset);
        draft.text.replace_range(draft.cursor..next, "");
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.move_focused(-3),
            MouseEventKind::ScrollDown => self.move_focused(3),
            MouseEventKind::Down(_) => {
                let position = Position::new(mouse.column, mouse.row);
                if self.hit_layout.drawer.contains(position) {
                    self.focus = FocusPane::Files;
                    let relative = usize::from(mouse.row.saturating_sub(self.hit_layout.drawer.y));
                    let index = self.drawer_scroll.saturating_add(relative);
                    if index < self.document.files.len() {
                        self.selected_file = index;
                        self.select_file_row();
                    }
                } else if self.hit_layout.patch.contains(position) {
                    self.focus = FocusPane::Diff;
                    self.select_clicked_row(mouse.row);
                }
            }
            _ => {}
        }
    }
}
