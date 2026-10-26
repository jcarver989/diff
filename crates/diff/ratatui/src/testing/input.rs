use crate::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind, ReviewInput};

#[must_use]
pub fn key(code: KeyCode) -> ReviewInput {
    key_with(code, KeyModifiers::NONE)
}

#[must_use]
pub fn key_with(code: KeyCode, modifiers: KeyModifiers) -> ReviewInput {
    ReviewInput::Key(KeyEvent::new(code, modifiers))
}

#[must_use]
pub fn mouse(kind: MouseEventKind, column: u16, row: u16) -> ReviewInput {
    ReviewInput::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}
