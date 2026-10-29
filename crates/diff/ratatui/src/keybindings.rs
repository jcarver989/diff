use crate::{
    DiffReviewCommand, KeyCode, KeyEvent, KeyModifiers, MarkdownFocusPane, MarkdownReviewCommand, ReviewCommand,
};
pub use clankerdiff_core::KeyScope as BindingScope;
use clankerdiff_core::{Chord, Key, KeyState, diff_shortcuts};
use std::{fmt::Write, ptr};

const NAMED_KEYS: [(Key, KeyCode); 11] = [
    (Key::Enter, KeyCode::Enter),
    (Key::Esc, KeyCode::Esc),
    (Key::Tab, KeyCode::Tab),
    (Key::Up, KeyCode::Up),
    (Key::Down, KeyCode::Down),
    (Key::Left, KeyCode::Left),
    (Key::Right, KeyCode::Right),
    (Key::Home, KeyCode::Home),
    (Key::End, KeyCode::End),
    (Key::PageUp, KeyCode::PageUp),
    (Key::PageDown, KeyCode::PageDown),
];
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBinding<T> {
    pub key: KeyEvent,
    pub command: T,
    pub label: String,
    pub scope: BindingScope,
}

impl<T> KeyBinding<T> {
    pub fn new(key: KeyEvent, command: T, label: impl Into<String>) -> Self {
        Self { key, command, label: label.into(), scope: BindingScope::Any }
    }

    #[must_use]
    pub fn with_scope(mut self, scope: BindingScope) -> Self {
        self.scope = scope;
        self
    }

    #[must_use]
    pub fn key_label(&self) -> String {
        let mut key = String::new();
        for (modifier, name) in [
            (KeyModifiers::CONTROL, "Ctrl-"),
            (KeyModifiers::ALT, "Alt-"),
            (KeyModifiers::SUPER, "Super-"),
            (KeyModifiers::HYPER, "Hyper-"),
            (KeyModifiers::META, "Meta-"),
            (KeyModifiers::SHIFT, "Shift-"),
        ] {
            if self.key.modifiers.contains(modifier) {
                key.push_str(name);
            }
        }
        let ctrl = self.key.modifiers.contains(KeyModifiers::CONTROL);
        match core_key(self.key.code) {
            Some(core) => key.push_str(&Chord { key: core, ctrl }.key_name()),
            None => {
                let _ = write!(key, "{:?}", self.key.code);
            }
        }
        key
    }

    pub fn hint(&self) -> String {
        format!("{} {}", self.key_label(), self.label)
    }
}

pub(crate) fn binding_in_state<T>(
    bindings: &[KeyBinding<T>],
    key: KeyEvent,
    state: KeyState,
) -> Option<&KeyBinding<T>> {
    let candidates = bindings.iter().filter(|binding| normalize_key(binding.key) == normalize_key(key));
    BindingScope::resolve(state, candidates.map(|binding| (binding.scope, binding)))
}

pub(crate) fn help_bindings<T>(
    bindings: &[KeyBinding<T>],
    navigation_available: bool,
    enabled: impl Fn(&T) -> bool,
) -> impl Iterator<Item = &KeyBinding<T>> {
    bindings.iter().filter(move |binding| {
        enabled(&binding.command)
            && [(true, false), (true, true), (false, false)]
                .into_iter()
                .filter(|(document, _)| *document || navigation_available)
                .any(|(document, split)| {
                    binding_in_state(bindings, binding.key, KeyState { document, split, ..KeyState::default() })
                        .is_some_and(|resolved| ptr::eq(resolved, *binding))
                })
    })
}

pub(crate) fn footer_hint<T: PartialEq>(
    bindings: &[KeyBinding<T>],
    document: bool,
    split: bool,
    enabled: impl Fn(&T) -> bool,
    help: &T,
    width: usize,
) -> String {
    let active: Vec<_> = bindings
        .iter()
        .filter(|binding| {
            enabled(&binding.command)
                && binding_in_state(bindings, binding.key, KeyState { document, split, ..KeyState::default() })
                    .is_some_and(|resolved| ptr::eq(resolved, *binding))
        })
        .collect();
    let help = active.iter().find(|binding| &binding.command == help).copied();
    let help_hint = help.map(KeyBinding::hint).filter(|hint| hint.width() <= width);
    let mut remaining = width.saturating_sub(help_hint.as_ref().map_or(0, |hint| hint.width() + 2));
    let mut commands = Vec::new();
    let mut hints = Vec::new();
    for binding in active {
        if help.is_some_and(|help| binding.command == help.command) || commands.contains(&&binding.command) {
            continue;
        }
        let hint = binding.hint();
        let needed = hint.width() + if hints.is_empty() { 0 } else { 2 };
        if hints.len() == 5 || needed > remaining {
            continue;
        }
        remaining -= needed;
        commands.push(&binding.command);
        hints.push(hint);
    }
    hints.extend(help_hint);
    hints.join("  ")
}

fn normalize_key(mut key: KeyEvent) -> KeyEvent {
    if let KeyCode::Char(c) = key.code
        && !c.is_lowercase()
    {
        key.modifiers.remove(KeyModifiers::SHIFT);
    }
    key
}

#[must_use]
pub fn default_diff_keybindings() -> Vec<KeyBinding<DiffReviewCommand>> {
    diff_shortcuts()
        .into_iter()
        .map(|(chord, scope, entry)| {
            KeyBinding::new(key_event(chord), entry.command.clone(), entry.title).with_scope(scope)
        })
        .collect()
}

fn key_event(chord: Chord) -> KeyEvent {
    let code = match chord.key {
        Key::Char(character) => KeyCode::Char(character),
        key => NAMED_KEYS.iter().find(|(named, _)| *named == key).map_or(KeyCode::Null, |(_, code)| *code),
    };
    KeyEvent::new(code, if chord.ctrl { KeyModifiers::CONTROL } else { KeyModifiers::NONE })
}

fn core_key(code: KeyCode) -> Option<Key> {
    match code {
        KeyCode::Char(character) => Some(Key::Char(character)),
        code => NAMED_KEYS.iter().find(|(_, named)| *named == code).map(|(key, _)| *key),
    }
}

pub(crate) fn markdown_command_label(command: MarkdownReviewCommand) -> &'static str {
    use MarkdownReviewCommand as C;
    use ReviewCommand as R;
    match command {
        C::Review(command) => match command {
            R::BeginComment => "comment",
            R::EditComment => "edit comment",
            R::DeleteComment => "delete comment",
            R::UndoComment => "undo comment",
            R::SubmitComment => "submit comment",
            R::Cancel => "cancel",
            R::ShowHelp => "help",
            R::ScrollHelp(_) => "scroll help",
            R::OpenThemePicker => "theme",
            R::SelectTheme(_) => "select theme",
            R::MoveTheme(_) => "move theme",
            R::CommitTheme => "apply theme",
        },
        C::Focus(MarkdownFocusPane::Document) => "document",
        C::Focus(MarkdownFocusPane::Outline) => "outline",
        C::ToggleFocus => "switch pane",
        C::SelectTarget(_) => "select target",
        C::SelectHeading(_) => "select heading",
        C::MoveSelection(lines) => {
            if lines < 0 {
                "previous"
            } else {
                "next"
            }
        }
        C::Scroll { .. } => "scroll",
        C::Page(pages) => {
            if pages < 0 {
                "page up"
            } else {
                "page down"
            }
        }
        C::First => "first",
        C::Last => "last",
        C::NextHeading => "next heading",
        C::PreviousHeading => "previous heading",
        C::OpenSelected => "open",
        C::Approve => "approve",
        C::RequestChanges => "request changes",
        C::CopyReview(_) => "copy",
    }
}

#[must_use]
pub fn default_markdown_keybindings() -> Vec<KeyBinding<MarkdownReviewCommand>> {
    use KeyCode as K;
    use MarkdownReviewCommand as C;
    let entries = [
        (K::Esc, ReviewCommand::Cancel.into()),
        (K::Tab, C::ToggleFocus),
        (K::Up, C::MoveSelection(-1)),
        (K::Char('k'), C::MoveSelection(-1)),
        (K::Down, C::MoveSelection(1)),
        (K::Char('j'), C::MoveSelection(1)),
        (K::Home, C::First),
        (K::Char('g'), C::First),
        (K::End, C::Last),
        (K::Char('G'), C::Last),
        (K::PageUp, C::Page(-1)),
        (K::PageDown, C::Page(1)),
        (K::Char('n'), C::NextHeading),
        (K::Char('p'), C::PreviousHeading),
        (K::Left, C::Focus(MarkdownFocusPane::Outline)),
        (K::Char('h'), C::Focus(MarkdownFocusPane::Outline)),
        (K::Right, C::Focus(MarkdownFocusPane::Document)),
        (K::Char('l'), C::Focus(MarkdownFocusPane::Document)),
        (K::Enter, C::OpenSelected),
        (K::Char('c'), ReviewCommand::BeginComment.into()),
        (K::Char('e'), ReviewCommand::EditComment.into()),
        (K::Char('x'), ReviewCommand::DeleteComment.into()),
        (K::Char('u'), ReviewCommand::UndoComment.into()),
        (K::Char('a'), C::Approve),
        (K::Char('r'), C::RequestChanges),
        (K::Char('t'), ReviewCommand::OpenThemePicker.into()),
        (K::Char('?'), ReviewCommand::ShowHelp.into()),
    ];
    entries
        .into_iter()
        .map(|(key, command)| (key.into(), command))
        .chain([(K::ctrl('g'), ReviewCommand::Cancel.into())])
        .map(|(key, command)| KeyBinding::new(key, command, markdown_command_label(command)))
        .collect()
}
