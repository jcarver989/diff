use crate::ReviewCommand;
use bitflags::bitflags;
use clankerdiff_core::CommentDraft;
pub use clankerdiff_core::InteractionPhase;
use clankerdiff_markdown::MarkdownCommentDraft;
use clankerdiff_theme::ThemeId;
use ratatui::layout::Position;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputOutcome<T> {
    Ignored,
    Consumed,
    Emitted(T),
    ThemeSelected(ThemeId),
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

impl KeyCode {
    #[must_use]
    pub fn ctrl(key: impl Into<Self>) -> KeyEvent {
        KeyEvent::new(key.into(), KeyModifiers::CONTROL)
    }

    #[must_use]
    pub fn alt(key: impl Into<Self>) -> KeyEvent {
        KeyEvent::new(key.into(), KeyModifiers::ALT)
    }

    #[must_use]
    pub fn shift(key: impl Into<Self>) -> KeyEvent {
        KeyEvent::new(key.into(), KeyModifiers::SHIFT)
    }
}

impl From<char> for KeyCode {
    fn from(key: char) -> Self {
        Self::Char(key)
    }
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

impl From<KeyCode> for KeyEvent {
    fn from(code: KeyCode) -> Self {
        Self::new(code, KeyModifiers::NONE)
    }
}

impl From<char> for KeyEvent {
    fn from(key: char) -> Self {
        KeyCode::from(key).into()
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

impl From<KeyEvent> for ReviewInput {
    fn from(key: KeyEvent) -> Self {
        Self::Key(key)
    }
}

impl From<KeyCode> for ReviewInput {
    fn from(key: KeyCode) -> Self {
        Self::Key(key.into())
    }
}

impl From<char> for ReviewInput {
    fn from(key: char) -> Self {
        Self::Key(key.into())
    }
}

impl From<MouseEvent> for ReviewInput {
    fn from(mouse: MouseEvent) -> Self {
        Self::Mouse(mouse)
    }
}

pub(crate) trait DraftEditor {
    fn insert(&mut self, text: &str);
    fn edit(&mut self, code: KeyCode);
}

impl DraftEditor for CommentDraft {
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

impl DraftEditor for MarkdownCommentDraft {
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

pub(crate) trait ReviewWidget {
    type Event;
    type Error;
    type Draft: DraftEditor;
    fn phase(&self) -> InteractionPhase;
    fn handle_review_command(
        &mut self,
        command: ReviewCommand,
    ) -> Result<InputOutcome<Self::Event>, Self::Error>;
    fn contains(&self, position: Position) -> bool;
    fn mark_dirty(&mut self);
    fn draft_mut(&mut self) -> Option<&mut Self::Draft>;
    fn draft_changed(&mut self);
    fn handle_browse_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<InputOutcome<Self::Event>, Self::Error>;
    fn handle_mouse(&mut self, mouse: MouseEvent) -> InputOutcome<Self::Event>;
    fn handle_prompt_key(&mut self, _key: KeyEvent) -> InputOutcome<Self::Event> {
        InputOutcome::Consumed
    }
    fn paste_prompt(&mut self, _text: &str) {}
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
                state.draft_changed();
                state.mark_dirty();
                InputOutcome::Consumed
            } else if phase != InteractionPhase::Browse {
                if phase == InteractionPhase::RepositoryPrompt {
                    state.paste_prompt(&text);
                    state.mark_dirty();
                }
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
                let outcome = state.handle_mouse(mouse);
                if outcome.is_consumed() {
                    state.mark_dirty();
                }
                outcome
            }
        }
    };
    Ok(outcome)
}

fn handle_key<T: ReviewWidget>(
    state: &mut T,
    key: KeyEvent,
) -> Result<InputOutcome<T::Event>, T::Error> {
    let command = match state.phase() {
        InteractionPhase::ThemePicker => {
            if !is_plain_key(key) {
                return Ok(InputOutcome::Consumed);
            }
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => ReviewCommand::Cancel,
                KeyCode::Enter => ReviewCommand::CommitTheme,
                KeyCode::Up | KeyCode::Char('k') => ReviewCommand::MoveTheme(-1),
                KeyCode::Down | KeyCode::Char('j') => ReviewCommand::MoveTheme(1),
                KeyCode::Home | KeyCode::Char('g') => ReviewCommand::SelectTheme(0),
                KeyCode::End | KeyCode::Char('G') => ReviewCommand::MoveTheme(isize::MAX),
                _ => return Ok(InputOutcome::Consumed),
            }
        }
        InteractionPhase::RepositoryPrompt => {
            let outcome = state.handle_prompt_key(key);
            if outcome.is_consumed() {
                state.mark_dirty();
            }
            return Ok(outcome);
        }
        InteractionPhase::Help => {
            if !is_plain_key(key) {
                return Ok(InputOutcome::Consumed);
            }
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') => ReviewCommand::Cancel,
                KeyCode::Down | KeyCode::Char('j') => ReviewCommand::ScrollHelp(1),
                KeyCode::Up | KeyCode::Char('k') => ReviewCommand::ScrollHelp(-1),
                KeyCode::PageDown => ReviewCommand::ScrollHelp(10),
                KeyCode::PageUp => ReviewCommand::ScrollHelp(-10),
                KeyCode::Home => ReviewCommand::ScrollHelp(isize::MIN),
                KeyCode::End => ReviewCommand::ScrollHelp(isize::MAX),
                _ => return Ok(InputOutcome::Consumed),
            }
        }
        InteractionPhase::Draft => return handle_draft_key(state, key),
        InteractionPhase::Browse => return state.handle_browse_key(key),
    };
    state.handle_review_command(command)
}

pub(crate) fn is_plain_key(key: KeyEvent) -> bool {
    key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT
}

fn handle_draft_key<T: ReviewWidget>(
    state: &mut T,
    key: KeyEvent,
) -> Result<InputOutcome<T::Event>, T::Error> {
    if key.code == KeyCode::Esc {
        return state.handle_review_command(ReviewCommand::Cancel);
    } else if key.code == KeyCode::Enter && !key.modifiers.contains(KeyModifiers::SHIFT) {
        return state.handle_review_command(ReviewCommand::SubmitComment);
    } else if let Some(draft) = state.draft_mut() {
        match key.code {
            KeyCode::Enter => draft.insert("\n"),
            KeyCode::Char(character) if is_plain_key(key) => {
                let mut buffer = [0; 4];
                draft.insert(character.encode_utf8(&mut buffer));
            }
            code => draft.edit(code),
        }
        state.draft_changed();
        state.mark_dirty();
    }
    Ok(InputOutcome::Consumed)
}
