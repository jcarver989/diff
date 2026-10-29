use crate::{
    CommandContext, DiffReviewCommand, DiffScope, DiffSide, FocusPane, RevealAmount, ReviewCommand, ReviewSession,
    ViewMode,
};
use CommandSection::{Git, Navigation, Review, Symbols, View};
use DiffReviewCommand as C;
use KeyScope::{Any, Card, Document, Navigation as Files, Reference, Split, Unified};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Enter,
    Esc,
    Tab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chord {
    pub key: Key,
    pub ctrl: bool,
}

impl Chord {
    #[must_use]
    pub const fn new(key: Key) -> Self {
        Self { key, ctrl: false }
    }

    #[must_use]
    pub const fn char(character: char) -> Self {
        Self::new(Key::Char(character))
    }

    #[must_use]
    pub const fn ctrl(character: char) -> Self {
        Self { key: Key::Char(character), ctrl: true }
    }

    #[must_use]
    pub fn label(self) -> String {
        if self.ctrl { format!("Ctrl-{}", self.key_name()) } else { self.key_name() }
    }

    #[must_use]
    pub fn key_name(self) -> String {
        match self.key {
            Key::Char(' ') => "Space".to_owned(),
            Key::Char(character) if self.ctrl => character.to_ascii_uppercase().to_string(),
            Key::Char(character) => character.to_string(),
            Key::Up => "↑".to_owned(),
            Key::Down => "↓".to_owned(),
            Key::Left => "←".to_owned(),
            Key::Right => "→".to_owned(),
            Key::PageUp => "PgUp".to_owned(),
            Key::PageDown => "PgDn".to_owned(),
            key => format!("{key:?}"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum KeyScope {
    #[default]
    Any,
    Navigation,
    Document,
    Split,
    Unified,
    Card,
    Reference,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "Key contexts are independent flags")]
pub struct KeyState {
    pub document: bool,
    pub split: bool,
    pub card: bool,
    pub reference: bool,
}

impl KeyState {
    #[must_use]
    pub fn new(pane: FocusPane, session: &ReviewSession, card: bool) -> Self {
        Self {
            document: pane == FocusPane::Diff,
            split: session.selected_layout().is_split(),
            card,
            reference: session.in_reference(),
        }
    }
}

impl KeyScope {
    #[must_use]
    pub const fn matches(self, state: KeyState) -> bool {
        match self {
            Self::Any => true,
            Self::Navigation => !state.document,
            Self::Document => state.document,
            Self::Split => state.document && state.split,
            Self::Unified => state.document && !state.split,
            Self::Card => state.card,
            Self::Reference => state.reference && !state.card,
        }
    }

    #[must_use]
    pub const fn precedence(self) -> u8 {
        match self {
            Self::Any => 0,
            Self::Navigation | Self::Document => 1,
            Self::Split | Self::Unified => 2,
            Self::Reference => 3,
            Self::Card => 4,
        }
    }

    pub fn resolve<T>(state: KeyState, candidates: impl IntoIterator<Item = (Self, T)>) -> Option<T> {
        candidates
            .into_iter()
            .filter(|(scope, _)| scope.matches(state))
            .fold(None, |best: Option<(u8, T)>, (scope, item)| match best {
                Some((rank, _)) if rank > scope.precedence() => best,
                _ => Some((scope.precedence(), item)),
            })
            .map(|(_, item)| item)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommandSection {
    Navigation,
    View,
    Review,
    Git,
    Symbols,
}

impl CommandSection {
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Navigation => "Navigation",
            Self::View => "View",
            Self::Review => "Review",
            Self::Git => "Git",
            Self::Symbols => "Symbols",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandEntry {
    pub command: DiffReviewCommand,
    pub title: &'static str,
    pub section: CommandSection,
    pub keys: &'static [(Chord, KeyScope)],
}

impl CommandEntry {
    #[must_use]
    pub fn key_label(&self) -> Option<String> {
        join_key_labels(self.keys.iter().map(|(chord, _)| chord.label()))
    }
}

#[must_use]
pub fn join_key_labels(labels: impl IntoIterator<Item = String>) -> Option<String> {
    let mut unique: Vec<String> = Vec::new();
    for label in labels {
        if !unique.contains(&label) {
            unique.push(label);
        }
    }
    (!unique.is_empty()).then(|| unique.join(" / "))
}

pub const DRAFT_HINT: &str = "Enter save · Shift-Enter newline · Esc cancel";

#[must_use]
pub const fn browse_hint(state: KeyState, source_view: bool) -> &'static str {
    if state.card {
        "Enter go to definition · Tab next definition · Esc close"
    } else if source_view {
        "j/k line · o back to diff · s submit · y copy · ? commands"
    } else if !state.document {
        "j/k entry · h/l fold/open · Tab pane · ? commands"
    } else if state.split {
        "j/k line · ←/→ side · c comment · s submit · ? commands"
    } else {
        "j/k line · c comment · e/x edit/delete · s submit · y copy · ? commands"
    }
}

#[must_use]
pub fn diff_commands() -> &'static [CommandEntry] {
    DIFF_COMMANDS
}

pub fn palette_commands<'a>(
    context: &'a CommandContext,
    session: &'a ReviewSession,
) -> impl Iterator<Item = &'static CommandEntry> + 'a {
    DIFF_COMMANDS.iter().filter(|entry| {
        entry.command.in_palette() && entry.command.enabled(context) && session.command_available(&entry.command)
    })
}

#[must_use]
pub fn command_entry(command: &DiffReviewCommand) -> Option<&'static CommandEntry> {
    DIFF_COMMANDS.iter().find(|entry| &entry.command == command)
}

#[must_use]
pub fn diff_shortcuts() -> Vec<(Chord, KeyScope, &'static CommandEntry)> {
    let mut shortcuts: Vec<_> = DIFF_COMMANDS
        .iter()
        .flat_map(|entry| entry.keys.iter().map(move |(chord, scope)| (*chord, *scope, entry)))
        .collect();
    shortcuts.sort_by_key(|(_, scope, _)| scope.precedence());
    shortcuts
}

const fn entry(
    command: DiffReviewCommand,
    title: &'static str,
    section: CommandSection,
    keys: &'static [(Chord, KeyScope)],
) -> CommandEntry {
    CommandEntry { command, title, section, keys }
}

const fn named(key: Key, scope: KeyScope) -> (Chord, KeyScope) {
    (Chord::new(key), scope)
}

const fn letter(character: char, scope: KeyScope) -> (Chord, KeyScope) {
    (Chord::char(character), scope)
}

static DIFF_COMMANDS: &[CommandEntry] = &[
    entry(C::MoveSelection(1), "Next item", Navigation, &[letter('j', Any), named(Key::Down, Any)]),
    entry(C::MoveSelection(-1), "Previous item", Navigation, &[letter('k', Any), named(Key::Up, Any)]),
    entry(C::Page(1), "Page down", Navigation, &[named(Key::PageDown, Any)]),
    entry(C::Page(-1), "Page up", Navigation, &[named(Key::PageUp, Any)]),
    entry(C::First, "First item", Navigation, &[letter('g', Any), named(Key::Home, Any)]),
    entry(C::Last, "Last item", Navigation, &[letter('G', Any), named(Key::End, Any)]),
    entry(C::ToggleFocus, "Switch pane", Navigation, &[named(Key::Tab, Any)]),
    entry(
        C::Focus(FocusPane::Files),
        "Focus changed files",
        Navigation,
        &[letter('h', Document), named(Key::Left, Unified)],
    ),
    entry(C::CollapseSelected, "Fold directory", Navigation, &[letter('h', Files), named(Key::Left, Files)]),
    entry(
        C::OpenSelected,
        "Open file or directory",
        Navigation,
        &[letter('l', Files), named(Key::Right, Files), named(Key::Enter, Files)],
    ),
    entry(C::SelectSide(DiffSide::Old), "Select old side", Navigation, &[named(Key::Left, Split)]),
    entry(C::SelectSide(DiffSide::New), "Select new side", Navigation, &[named(Key::Right, Split)]),
    entry(C::RevealGap(RevealAmount::Step), "Expand context", Navigation, &[named(Key::Enter, Document)]),
    entry(C::StepTarget(-1), "Previous commit", Navigation, &[letter('[', Any)]),
    entry(C::StepTarget(1), "Next commit", Navigation, &[letter(']', Any)]),
    entry(C::OpenCommitPicker, "Choose commit", Navigation, &[letter('p', Any)]),
    entry(C::GoToPage(0), "First commit", Navigation, &[]),
    entry(C::GoToPage(usize::MAX), "Uncommitted changes", Navigation, &[]),
    entry(C::SetScope(DiffScope::Unstaged), "Show unstaged changes", Navigation, &[]),
    entry(C::SetScope(DiffScope::Staged), "Show staged changes", Navigation, &[]),
    entry(C::SetScope(DiffScope::Both), "Show all changes", Navigation, &[]),
    entry(C::ToggleSidebar, "Hide / show changed files", View, &[letter('b', Any)]),
    entry(C::ToggleSourceView, "Toggle source / diff", View, &[letter('o', Any)]),
    entry(C::ToggleFullFile, "Toggle all diff context", View, &[letter('f', Document)]),
    entry(C::RevealGap(RevealAmount::All), "Expand all context", View, &[letter('O', Document)]),
    entry(C::ToggleFileCollapsed(None), "Collapse / expand file", View, &[letter('z', Document)]),
    entry(C::ToggleAllFilesCollapsed, "Collapse / expand all files", View, &[letter('Z', Any)]),
    entry(C::ToggleLayout, "Toggle split / unified", View, &[letter('v', Any)]),
    entry(C::SetViewMode(ViewMode::Auto), "Auto layout", View, &[]),
    entry(C::SetViewMode(ViewMode::Unified), "Unified layout", View, &[]),
    entry(C::SetViewMode(ViewMode::Split), "Split layout", View, &[]),
    entry(C::Review(ReviewCommand::OpenThemePicker), "Choose theme", View, &[letter('t', Any)]),
    entry(C::Refresh, "Refresh repository", View, &[]),
    entry(C::Review(ReviewCommand::BeginComment), "Add comment", Review, &[letter('c', Document)]),
    entry(C::Review(ReviewCommand::EditComment), "Edit comment", Review, &[letter('e', Document)]),
    entry(C::Review(ReviewCommand::DeleteComment), "Delete comment", Review, &[letter('x', Document)]),
    entry(C::Review(ReviewCommand::UndoComment), "Undo last comment", Review, &[letter('u', Document)]),
    entry(C::SubmitReview, "Submit review", Review, &[letter('s', Document)]),
    entry(C::CopyReview, "Copy review", Review, &[letter('y', Document)]),
    entry(C::Review(ReviewCommand::Cancel), "Cancel review", Review, &[named(Key::Esc, Any)]),
    entry(C::Review(ReviewCommand::ShowHelp), "Commands", Review, &[letter('?', Any)]),
    entry(C::ToggleStage, "Stage / unstage", Git, &[letter(' ', Files)]),
    entry(C::StageAll, "Stage all", Git, &[letter('a', Files)]),
    entry(C::UnstageAll, "Unstage all", Git, &[letter('A', Files)]),
    entry(C::BeginCommit, "Commit staged changes", Git, &[letter('C', Any)]),
    entry(C::BeginDiscard, "Discard file changes", Git, &[letter('d', Any)]),
    entry(C::ShowDefinition, "Show definition", Symbols, &[letter('D', Document)]),
    entry(C::GoToDefinition, "Go to definition", Symbols, &[named(Key::Enter, Card)]),
    entry(C::NextDefinition, "Next definition", Symbols, &[named(Key::Tab, Card)]),
    entry(C::CloseDefinition, "Close definition", Symbols, &[named(Key::Esc, Card)]),
    entry(C::NavigateBack, "Back", Symbols, &[(Chord::ctrl('o'), Any), named(Key::Esc, Reference)]),
    entry(C::NavigateForward, "Forward", Symbols, &[]),
];
