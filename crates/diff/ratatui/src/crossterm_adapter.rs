use crate::{
    InputOutcome, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind, ReviewInput,
    interaction::{self, ReviewWidget},
};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, KeyboardEnhancementFlags, ModifierKeyCode,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement,
    },
};
use std::io::{self, Write};

pub struct TerminalSession<W: Write> {
    writer: W,
    keyboard_enhancement: bool,
}

impl<W: Write> TerminalSession<W> {
    pub fn enter(writer: W) -> io::Result<Self> {
        enable_raw_mode()?;
        let mut session = Self { writer, keyboard_enhancement: false };
        execute!(session.writer, EnterAlternateScreen, EnableMouseCapture)?;
        if supports_keyboard_enhancement().unwrap_or(false) {
            let flags = KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES;
            session.keyboard_enhancement = execute!(session.writer, PushKeyboardEnhancementFlags(flags)).is_ok();
        }
        Ok(session)
    }
}

impl<W: Write> Drop for TerminalSession<W> {
    fn drop(&mut self) {
        if self.keyboard_enhancement {
            let _ = execute!(self.writer, PopKeyboardEnhancementFlags);
        }
        let _ = disable_raw_mode();
        let _ = execute!(self.writer, DisableMouseCapture, LeaveAlternateScreen);
    }
}

pub(crate) fn handle_event<T: ReviewWidget>(state: &mut T, event: Event) -> Result<InputOutcome<T::Event>, T::Error> {
    let input = match event {
        Event::Key(key) if let event::KeyCode::Modifier(modifier) = key.code => {
            ReviewInput::Modifiers(held_modifiers(key, modifier))
        }
        Event::Key(key) if key.kind != KeyEventKind::Release => ReviewInput::Key(key.into()),
        Event::Paste(text) => ReviewInput::Paste(text),
        Event::Mouse(mouse) => ReviewInput::Mouse(mouse.into()),
        Event::Resize(..) => {
            state.mark_dirty();
            return Ok(InputOutcome::Ignored);
        }
        _ => return Ok(InputOutcome::Ignored),
    };
    interaction::handle_input(state, input)
}

fn held_modifiers(key: event::KeyEvent, modifier: ModifierKeyCode) -> KeyModifiers {
    let mut held = KeyModifiers::from(key.modifiers);
    let changed = match modifier {
        ModifierKeyCode::LeftShift | ModifierKeyCode::RightShift => KeyModifiers::SHIFT,
        ModifierKeyCode::LeftControl | ModifierKeyCode::RightControl => KeyModifiers::CONTROL,
        ModifierKeyCode::LeftAlt | ModifierKeyCode::RightAlt => KeyModifiers::ALT,
        ModifierKeyCode::LeftSuper | ModifierKeyCode::RightSuper => KeyModifiers::SUPER,
        ModifierKeyCode::LeftHyper | ModifierKeyCode::RightHyper => KeyModifiers::HYPER,
        ModifierKeyCode::LeftMeta | ModifierKeyCode::RightMeta => KeyModifiers::META,
        ModifierKeyCode::IsoLevel3Shift | ModifierKeyCode::IsoLevel5Shift => KeyModifiers::NONE,
    };
    held.set(changed, key.kind != KeyEventKind::Release);
    held
}

impl From<event::KeyModifiers> for KeyModifiers {
    fn from(value: event::KeyModifiers) -> Self {
        let mut result = Self::NONE;
        for (source, target) in [
            (event::KeyModifiers::SHIFT, Self::SHIFT),
            (event::KeyModifiers::CONTROL, Self::CONTROL),
            (event::KeyModifiers::ALT, Self::ALT),
            (event::KeyModifiers::SUPER, Self::SUPER),
            (event::KeyModifiers::HYPER, Self::HYPER),
            (event::KeyModifiers::META, Self::META),
        ] {
            if value.contains(source) {
                result |= target;
            }
        }
        result
    }
}

impl From<event::KeyCode> for KeyCode {
    fn from(value: event::KeyCode) -> Self {
        match value {
            event::KeyCode::Char(c) => Self::Char(c),
            event::KeyCode::F(n) => Self::F(n),
            event::KeyCode::Enter => Self::Enter,
            event::KeyCode::Esc => Self::Esc,
            event::KeyCode::Tab => Self::Tab,
            event::KeyCode::BackTab => Self::BackTab,
            event::KeyCode::Backspace => Self::Backspace,
            event::KeyCode::Delete => Self::Delete,
            event::KeyCode::Insert => Self::Insert,
            event::KeyCode::Left => Self::Left,
            event::KeyCode::Right => Self::Right,
            event::KeyCode::Up => Self::Up,
            event::KeyCode::Down => Self::Down,
            event::KeyCode::Home => Self::Home,
            event::KeyCode::End => Self::End,
            event::KeyCode::PageUp => Self::PageUp,
            event::KeyCode::PageDown => Self::PageDown,
            _ => Self::Null,
        }
    }
}

impl From<event::KeyEvent> for KeyEvent {
    fn from(value: event::KeyEvent) -> Self {
        Self::new(value.code.into(), value.modifiers.into())
    }
}

impl From<event::MouseButton> for MouseButton {
    fn from(value: event::MouseButton) -> Self {
        match value {
            event::MouseButton::Left => Self::Left,
            event::MouseButton::Right => Self::Right,
            event::MouseButton::Middle => Self::Middle,
        }
    }
}
impl From<event::MouseEventKind> for MouseEventKind {
    fn from(value: event::MouseEventKind) -> Self {
        match value {
            event::MouseEventKind::Down(button) => Self::Down(button.into()),
            event::MouseEventKind::Up(button) => Self::Up(button.into()),
            event::MouseEventKind::Drag(button) => Self::Drag(button.into()),
            event::MouseEventKind::Moved => Self::Moved,
            event::MouseEventKind::ScrollUp => Self::ScrollUp,
            event::MouseEventKind::ScrollDown => Self::ScrollDown,
            event::MouseEventKind::ScrollLeft => Self::ScrollLeft,
            event::MouseEventKind::ScrollRight => Self::ScrollRight,
        }
    }
}
impl From<event::MouseEvent> for MouseEvent {
    fn from(value: event::MouseEvent) -> Self {
        Self { kind: value.kind.into(), column: value.column, row: value.row, modifiers: value.modifiers.into() }
    }
}
