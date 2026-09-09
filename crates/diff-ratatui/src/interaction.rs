use crate::theme_picker::{self, ThemePicker};
use bitflags::bitflags;
use clankerdiff_core::CommentDraft;
use clankerdiff_markdown::MarkdownCommentDraft;
use clankerdiff_theme::ReviewTheme;
use ratatui::layout::Position;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputOutcome<T> {
    Ignored,
    Consumed,
    Emitted(T),
}

impl<T> InputOutcome<T> {
    #[must_use]
    pub const fn is_consumed(&self) -> bool {
        !matches!(self, Self::Ignored)
    }
    #[must_use]
    pub fn into_event(self) -> Option<T> {
        match self {
            Self::Emitted(event) => Some(event),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionPhase {
    Browse,
    Draft,
    Help,
    ThemePicker,
    RepositoryPrompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Insert,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
    Null,
}

bitflags! {
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct KeyModifiers: u8 {
        const NONE = 0;
        const SHIFT = 1;
        const CONTROL = 2;
        const ALT = 4;
        const SUPER = 8;
        const HYPER = 16;
        const META = 32;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}
impl KeyEvent {
    #[must_use]
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEventKind {
    Down(MouseButton),
    Up(MouseButton),
    Drag(MouseButton),
    Moved,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub column: u16,
    pub row: u16,
    pub modifiers: KeyModifiers,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewInput {
    Key(KeyEvent),
    Paste(String),
    Mouse(MouseEvent),
}

pub(crate) trait DraftEditor {
    fn insert(&mut self, text: &str);
    fn edit(&mut self, code: KeyCode);
}

macro_rules! draft_editor {
    ($t:ty) => {
        impl DraftEditor for $t {
            fn insert(&mut self, text: &str) {
                Self::insert(self, text);
            }
            fn edit(&mut self, code: KeyCode) {
                match code {
                    KeyCode::Left => self.move_cursor_left(),
                    KeyCode::Right => self.move_cursor_right(),
                    KeyCode::Home => self.move_cursor_to_start(),
                    KeyCode::End => self.move_cursor_to_end(),
                    KeyCode::Backspace => self.delete_before_cursor(),
                    KeyCode::Delete => self.delete_at_cursor(),
                    _ => {}
                }
            }
        }
    };
}
draft_editor!(CommentDraft);
draft_editor!(MarkdownCommentDraft);

pub(crate) trait ReviewWidget {
    type Event;
    type Error;
    type Draft: DraftEditor;
    fn phase(&self) -> InteractionPhase;
    fn contains(&self, position: Position) -> bool;
    fn mark_dirty(&mut self);
    fn draft_mut(&mut self) -> Option<&mut Self::Draft>;
    fn cancel_draft(&mut self);
    fn submit_draft(&mut self);
    fn draft_changed(&mut self, closed: bool);
    fn theme_picker(&mut self) -> &mut Option<ThemePicker>;
    fn set_theme(&mut self, theme: ReviewTheme);
    fn close_help(&mut self);
    fn cancel_event() -> Self::Event;
    fn handle_browse_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<InputOutcome<Self::Event>, Self::Error>;
    fn handle_mouse(&mut self, mouse: MouseEvent) -> InputOutcome<Self::Event>;
    fn handle_prompt_key(&mut self, _key: KeyEvent) -> InputOutcome<Self::Event> {
        InputOutcome::Consumed
    }
}

pub(crate) fn handle_input<T: ReviewWidget>(
    state: &mut T,
    input: ReviewInput,
) -> Result<InputOutcome<T::Event>, T::Error> {
    let phase = state.phase();
    let outcome = match input {
        ReviewInput::Key(key) => handle_key(state, key)?,
        ReviewInput::Paste(text) => {
            if let Some(draft) = state.draft_mut() {
                draft.insert(&text);
                state.draft_changed(false);
                InputOutcome::Consumed
            } else if phase != InteractionPhase::Browse {
                InputOutcome::Consumed
            } else {
                InputOutcome::Ignored
            }
        }
        ReviewInput::Mouse(mouse) => {
            if phase != InteractionPhase::Browse {
                InputOutcome::Consumed
            } else if !state.contains(Position::new(mouse.column, mouse.row))
                || !matches!(
                    mouse.kind,
                    MouseEventKind::Down(_) | MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                )
            {
                InputOutcome::Ignored
            } else {
                state.handle_mouse(mouse)
            }
        }
    };
    if outcome.is_consumed() {
        state.mark_dirty();
    }
    Ok(outcome)
}

fn handle_key<T: ReviewWidget>(
    state: &mut T,
    key: KeyEvent,
) -> Result<InputOutcome<T::Event>, T::Error> {
    match state.phase() {
        InteractionPhase::ThemePicker => {
            if let Some(theme) = theme_picker::apply_key(state.theme_picker(), key) {
                state.set_theme(theme);
            }
        }
        InteractionPhase::RepositoryPrompt => return Ok(state.handle_prompt_key(key)),
        InteractionPhase::Help => {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
                state.close_help();
            }
        }
        InteractionPhase::Draft => handle_draft_key(state, key),
        InteractionPhase::Browse => {
            if key.code == KeyCode::Esc
                || (key.code == KeyCode::Char('g') && key.modifiers.contains(KeyModifiers::CONTROL))
            {
                return Ok(InputOutcome::Emitted(T::cancel_event()));
            }
            if key.modifiers.intersects(
                KeyModifiers::ALT
                    | KeyModifiers::SUPER
                    | KeyModifiers::CONTROL
                    | KeyModifiers::HYPER
                    | KeyModifiers::META,
            ) {
                return Ok(InputOutcome::Ignored);
            }
            return state.handle_browse_key(key);
        }
    }
    Ok(InputOutcome::Consumed)
}

fn handle_draft_key<T: ReviewWidget>(state: &mut T, key: KeyEvent) {
    if key.code == KeyCode::Esc {
        state.cancel_draft();
        state.draft_changed(true);
    } else if key.code == KeyCode::Enter && !key.modifiers.contains(KeyModifiers::SHIFT) {
        state.submit_draft();
        let closed = state.draft_mut().is_none();
        state.draft_changed(closed);
    } else if let Some(draft) = state.draft_mut() {
        match key.code {
            KeyCode::Enter => draft.insert("\n"),
            KeyCode::Char(character)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                let mut buffer = [0; 4];
                draft.insert(character.encode_utf8(&mut buffer));
            }
            code => draft.edit(code),
        }
        state.draft_changed(false);
    }
}
