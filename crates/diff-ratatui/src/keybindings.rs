use crate::{
    DiffReviewCommand, FocusPane, KeyCode, KeyEvent, KeyModifiers, MarkdownFocusPane,
    MarkdownReviewCommand, ReviewCommand,
};
use clankerdiff_core::{DiffSide, RevealAmount};
use std::{fmt::Write, ptr};
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BindingScope {
    #[default]
    Any,
    Navigation,
    Document,
    SplitDocument,
}

impl BindingScope {
    pub(crate) fn matches(self, document: bool, split: bool) -> bool {
        match self {
            Self::Any => true,
            Self::Navigation => !document,
            Self::Document => document,
            Self::SplitDocument => document && split,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBinding<T> {
    pub key: KeyEvent,
    pub command: T,
    pub label: String,
    pub scope: BindingScope,
}

impl<T> KeyBinding<T> {
    pub fn new(key: KeyEvent, command: T, label: impl Into<String>) -> Self {
        Self {
            key,
            command,
            label: label.into(),
            scope: BindingScope::Any,
        }
    }

    #[must_use]
    pub fn with_scope(mut self, scope: BindingScope) -> Self {
        self.scope = scope;
        self
    }

    pub(crate) fn matches(&self, key: KeyEvent, document: bool, split: bool) -> bool {
        normalize_key(self.key) == normalize_key(key) && self.scope.matches(document, split)
    }

    pub fn hint(&self) -> String {
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
        match self.key.code {
            KeyCode::Char(' ') => key.push_str("Space"),
            KeyCode::Char(c) => key.push(c),
            code => {
                let _ = write!(key, "{code:?}");
            }
        }
        format!("{key} {}", self.label)
    }
}

pub(crate) fn binding_for_key<T>(
    bindings: &[KeyBinding<T>],
    key: KeyEvent,
    document: bool,
    split: bool,
) -> Option<&KeyBinding<T>> {
    bindings
        .iter()
        .rev()
        .find(|binding| binding.matches(key, document, split))
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
                    binding_for_key(bindings, binding.key, document, split)
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
                && binding_for_key(bindings, binding.key, document, split)
                    .is_some_and(|resolved| ptr::eq(resolved, *binding))
        })
        .collect();
    let help = active
        .iter()
        .find(|binding| &binding.command == help)
        .copied();
    let help_hint = help
        .map(KeyBinding::hint)
        .filter(|hint| hint.width() <= width);
    let mut remaining = width.saturating_sub(help_hint.as_ref().map_or(0, |hint| hint.width() + 2));
    let mut commands = Vec::new();
    let mut hints = Vec::new();
    for binding in active {
        if help.is_some_and(|help| binding.command == help.command)
            || commands.contains(&&binding.command)
        {
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
#[expect(
    clippy::too_many_lines,
    reason = "Declarative table of default keybindings"
)]
pub fn default_diff_keybindings() -> Vec<KeyBinding<DiffReviewCommand>> {
    use BindingScope::{Any, Document, Navigation, SplitDocument};
    use DiffReviewCommand as C;
    use KeyCode as K;
    let entries = [
        (K::Esc, ReviewCommand::Cancel.into(), "cancel", Any),
        (K::Tab, C::ToggleFocus, "switch pane", Any),
        (K::Left, C::Focus(FocusPane::Files), "files", Document),
        (K::Char('h'), C::Focus(FocusPane::Files), "files", Document),
        (K::Left, C::CollapseSelected, "collapse", Navigation),
        (K::Char('h'), C::CollapseSelected, "collapse", Navigation),
        (K::Right, C::OpenSelected, "open", Navigation),
        (K::Char('l'), C::OpenSelected, "open", Navigation),
        (K::Enter, C::OpenSelected, "open", Navigation),
        (K::Up, C::MoveSelection(-1), "previous", Any),
        (K::Char('k'), C::MoveSelection(-1), "previous", Any),
        (K::Down, C::MoveSelection(1), "next", Any),
        (K::Char('j'), C::MoveSelection(1), "next", Any),
        (K::PageUp, C::Page(-1), "page up", Any),
        (K::PageDown, C::Page(1), "page down", Any),
        (K::Home, C::First, "first", Any),
        (K::End, C::Last, "last", Any),
        (
            K::Char('c'),
            ReviewCommand::BeginComment.into(),
            "comment",
            Document,
        ),
        (
            K::Char('e'),
            ReviewCommand::EditComment.into(),
            "edit comment",
            Document,
        ),
        (
            K::Char('x'),
            ReviewCommand::DeleteComment.into(),
            "delete comment",
            Document,
        ),
        (
            K::Char('u'),
            ReviewCommand::UndoComment.into(),
            "undo comment",
            Document,
        ),
        (K::Char('s'), C::SubmitReview, "submit", Document),
        (K::Char('y'), C::CopyReview, "copy", Document),
        (K::Char(' '), C::ToggleStage, "stage/unstage", Navigation),
        (K::Char('a'), C::StageAll, "stage all", Navigation),
        (K::Char('A'), C::UnstageAll, "unstage all", Navigation),
        (K::Char('C'), C::BeginCommit, "commit", Any),
        (K::Char('d'), C::BeginDiscard, "discard", Any),
        (
            K::Char('t'),
            ReviewCommand::OpenThemePicker.into(),
            "theme",
            Any,
        ),
        (
            K::Enter,
            C::RevealGap(RevealAmount::Step),
            "reveal gap",
            Document,
        ),
        (
            K::Char('o'),
            C::RevealGap(RevealAmount::Step),
            "reveal gap",
            Document,
        ),
        (
            K::Char('O'),
            C::RevealGap(RevealAmount::All),
            "reveal all",
            Document,
        ),
        (K::Char('f'), C::ToggleFullFile, "full file", Document),
        (K::Char('v'), C::CycleViewMode, "view", Any),
        (K::Char('S'), C::CycleScope, "scope", Any),
        (K::Char('r'), C::Refresh, "refresh", Any),
        (K::Char('?'), ReviewCommand::ShowHelp.into(), "help", Any),
        (
            K::Left,
            C::SelectSide(DiffSide::Old),
            "old side",
            SplitDocument,
        ),
        (
            K::Right,
            C::SelectSide(DiffSide::New),
            "new side",
            SplitDocument,
        ),
    ];
    let mut bindings: Vec<_> = entries
        .into_iter()
        .map(|(key, command, label, scope)| {
            KeyBinding::new(KeyEvent::new(key, KeyModifiers::NONE), command, label)
                .with_scope(scope)
        })
        .collect();
    bindings.push(KeyBinding::new(
        KeyEvent::new(K::Char('g'), KeyModifiers::CONTROL),
        ReviewCommand::Cancel.into(),
        "cancel",
    ));
    bindings
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
