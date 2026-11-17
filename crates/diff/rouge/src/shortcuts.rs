use clankerdiff_core::{
    Chord as DomainChord, DiffReviewCommand, Key as DomainKey, KeyScope, KeyState, command_entry, diff_shortcuts,
    join_key_labels,
};
use rouge_core::{Chord, Key, KeyEvent, Modifiers};

fn rouge_chord(chord: DomainChord) -> Chord {
    let key = match chord.key {
        DomainKey::Char(' ') => Key::Space,
        DomainKey::Char('?') => Key::character("/"),
        DomainKey::Char(ch) => Key::character(&ch.to_string()),
        DomainKey::Enter => Key::Enter,
        DomainKey::Esc => Key::Escape,
        DomainKey::Tab => Key::Tab,
        DomainKey::Up => Key::ArrowUp,
        DomainKey::Down => Key::ArrowDown,
        DomainKey::Left => Key::ArrowLeft,
        DomainKey::Right => Key::ArrowRight,
        DomainKey::Home => Key::Home,
        DomainKey::End => Key::End,
        DomainKey::PageUp => Key::PageUp,
        DomainKey::PageDown => Key::PageDown,
    };
    let mut modifiers = if chord.ctrl { Modifiers::CTRL } else { Modifiers::NONE };
    if matches!(chord.key, DomainKey::Char(ch) if ch == '?' || ch.is_ascii_uppercase()) {
        modifiers = modifiers.with(Modifiers::SHIFT);
    }
    Chord::new(key, modifiers)
}

pub(crate) struct Shortcuts(Vec<(Chord, KeyScope, &'static DiffReviewCommand)>);

impl Shortcuts {
    pub(crate) fn new() -> Self {
        Self(
            diff_shortcuts().into_iter().map(|(key, scope, entry)| (rouge_chord(key), scope, &entry.command)).collect(),
        )
    }

    pub(crate) fn resolve(&self, event: &KeyEvent, state: KeyState) -> Option<&'static DiffReviewCommand> {
        KeyScope::resolve(
            state,
            self.0
                .iter()
                .filter(|(chord, _, _)| {
                    if matches!(&chord.key, Key::Character(key) if key == "/") {
                        return event.modifiers == Modifiers::SHIFT && chord.matches(event)
                            || event.text.as_deref() == Some("?")
                                && !event.modifiers.ctrl
                                && !event.modifiers.alt
                                && !event.modifiers.meta;
                    }
                    chord.matches(event) && chord.modifiers.shift == event.modifiers.shift
                })
                .map(|(_, scope, command)| (*scope, *command)),
        )
    }
}

#[must_use]
pub fn shortcut_label(command: &DiffReviewCommand) -> Option<String> {
    join_key_labels(command_entry(command)?.keys.iter().map(|(chord, _)| chord.label()))
}
