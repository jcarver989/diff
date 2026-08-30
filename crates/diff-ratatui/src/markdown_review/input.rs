use super::{MarkdownFocusPane, MarkdownReviewEvent, MarkdownReviewState};
use crate::theme_picker::{ThemePicker, ThemePickerAction};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use diff_markdown::MarkdownReviewError;
use ratatui::layout::Position;

/// Framework-neutral input accepted by [`MarkdownReviewState::handle_input`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownReviewInput {
    /// A Crossterm key event.
    Key(KeyEvent),
    /// Text pasted into the active draft.
    Paste(String),
    /// A Crossterm mouse event.
    Mouse(MouseEvent),
}

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
pub fn handle_crossterm_event(
    state: &mut MarkdownReviewState,
    event: Event,
) -> Result<Option<MarkdownReviewEvent>, MarkdownReviewError> {
    match event {
        Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            state.handle_input(MarkdownReviewInput::Key(key))
        }
        Event::Paste(text) => state.handle_input(MarkdownReviewInput::Paste(text)),
        Event::Mouse(mouse) => state.handle_input(MarkdownReviewInput::Mouse(mouse)),
        Event::Resize(..) => {
            state.mark_dirty();
            Ok(None)
        }
        _ => Ok(None),
    }
}

impl MarkdownReviewState {
    /// Applies one input event.
    ///
    /// # Errors
    ///
    /// Returns [`MarkdownReviewError::BlankComment`] when an approval or
    /// request-changes action encounters a blank comment body.
    pub fn handle_input(
        &mut self,
        input: MarkdownReviewInput,
    ) -> Result<Option<MarkdownReviewEvent>, MarkdownReviewError> {
        match input {
            MarkdownReviewInput::Key(key) => {
                self.mark_dirty();
                self.handle_key(key)
            }
            MarkdownReviewInput::Paste(text) => {
                if let Some(draft) = self.session.draft_mut() {
                    draft.insert(&text);
                    self.request_follow();
                }
                Ok(None)
            }
            MarkdownReviewInput::Mouse(mouse) => {
                self.handle_mouse(mouse);
                Ok(None)
            }
        }
    }

    fn handle_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<Option<MarkdownReviewEvent>, MarkdownReviewError> {
        if let Some(picker) = self.theme_picker.as_mut() {
            let action = picker.handle_key(key);
            match action {
                ThemePickerAction::Preview(theme) => self.set_theme(theme),
                ThemePickerAction::Restore(theme) => {
                    self.set_theme(theme);
                    self.theme_picker = None;
                }
                ThemePickerAction::Commit => self.theme_picker = None,
                ThemePickerAction::None => {}
            }
            return Ok(None);
        }
        if self.help {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
                self.help = false;
            }
            return Ok(None);
        }
        if self.session.draft().is_some() {
            return Ok(self.handle_draft_key(key));
        }
        if key.code == KeyCode::Esc
            || (key.code == KeyCode::Char('g') && key.modifiers.contains(KeyModifiers::CONTROL))
        {
            return Ok(Some(MarkdownReviewEvent::Cancel));
        }
        if key
            .modifiers
            .intersects(KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::CONTROL)
        {
            return Ok(None);
        }
        self.handle_browse_key(key)
    }

    fn handle_browse_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<Option<MarkdownReviewEvent>, MarkdownReviewError> {
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
            KeyCode::Char('a') => return self.session.approve().map(Some),
            KeyCode::Char('r') => return self.session.request_changes().map(Some),
            KeyCode::Char('t') => {
                self.theme_picker = Some(ThemePicker::new(&self.theme));
            }
            KeyCode::Char('?') => self.help = true,
            _ => {}
        }
        self.sync_outline_selection();
        self.request_follow();
        Ok(None)
    }

    fn handle_draft_key(&mut self, key: KeyEvent) -> Option<MarkdownReviewEvent> {
        if key.code == KeyCode::Esc {
            self.session.cancel_draft();
            self.request_follow();
            return None;
        }
        if key.code == KeyCode::Enter && !key.modifiers.contains(KeyModifiers::SHIFT) {
            self.session.submit_draft();
            self.request_follow();
            return None;
        }
        let draft = self.session.draft_mut()?;
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
        self.request_follow();
        None
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
