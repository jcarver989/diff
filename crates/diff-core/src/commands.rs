use crate::{DiffScope, DiffSide, RepositoryAction, RevealAmount, ViewMode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusPane {
    #[default]
    Files,
    Diff,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InteractionPhase {
    #[default]
    Browse,
    Draft,
    Help,
    ThemePicker,
    RepositoryPrompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReviewCapabilities {
    pub repository: bool,
    pub refresh: bool,
    pub scope: bool,
    pub submit: bool,
    pub clipboard: bool,
}

impl Default for ReviewCapabilities {
    fn default() -> Self {
        Self {
            repository: true,
            refresh: true,
            scope: true,
            submit: true,
            clipboard: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandContext {
    pub phase: InteractionPhase,
    pub capabilities: ReviewCapabilities,
    pub repository_pending: bool,
    pub document_ready: bool,
    pub navigation_available: bool,
    pub themes_available: bool,
}

impl Default for CommandContext {
    fn default() -> Self {
        Self {
            phase: InteractionPhase::Browse,
            capabilities: ReviewCapabilities::default(),
            repository_pending: false,
            document_ready: true,
            navigation_available: true,
            themes_available: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewCommand {
    BeginComment,
    EditComment,
    DeleteComment,
    UndoComment,
    SubmitComment,
    Cancel,
    ShowHelp,
    ScrollHelp(isize),
    OpenThemePicker,
    SelectTheme(usize),
    MoveTheme(isize),
    CommitTheme,
}

impl ReviewCommand {
    #[must_use]
    pub fn enabled(self, context: &CommandContext) -> bool {
        let phase = context.phase;
        match self {
            Self::Cancel => true,
            Self::ShowHelp => phase == InteractionPhase::Browse,
            Self::ScrollHelp(_) => phase == InteractionPhase::Help,
            Self::OpenThemePicker => phase == InteractionPhase::Browse && context.themes_available,
            Self::SelectTheme(_) | Self::MoveTheme(_) | Self::CommitTheme => {
                phase == InteractionPhase::ThemePicker
            }
            Self::SubmitComment => phase == InteractionPhase::Draft && context.document_ready,
            Self::BeginComment | Self::EditComment | Self::DeleteComment | Self::UndoComment => {
                phase == InteractionPhase::Browse && context.document_ready
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffReviewCommand {
    Review(ReviewCommand),
    Focus(FocusPane),
    ToggleFocus,
    SelectFile(usize),
    SelectRow(usize),
    SelectSide(DiffSide),
    MoveSelection(isize),
    Scroll { pane: FocusPane, lines: isize },
    Page(isize),
    First,
    Last,
    OpenSelected,
    CollapseSelected,
    RevealGap(RevealAmount),
    ToggleFullFile,
    SetViewMode(ViewMode),
    CycleViewMode,
    SubmitReview,
    CopyReview,
    SetScope(DiffScope),
    CycleScope,
    Refresh,
    ToggleStage,
    StageAll,
    UnstageAll,
    BeginCommit,
    BeginDiscard,
    RepositoryAction(RepositoryAction),
}

impl From<ReviewCommand> for DiffReviewCommand {
    fn from(command: ReviewCommand) -> Self {
        Self::Review(command)
    }
}

impl DiffReviewCommand {
    #[must_use]
    pub fn enabled(&self, context: &CommandContext) -> bool {
        if let Self::Review(command) = self {
            return command.enabled(context);
        }
        if context.phase != InteractionPhase::Browse {
            return false;
        }
        let capability = match self {
            Self::SetScope(_) | Self::CycleScope => {
                context.capabilities.scope && !context.repository_pending
            }
            Self::Refresh => context.capabilities.refresh && !context.repository_pending,
            Self::ToggleStage
            | Self::StageAll
            | Self::UnstageAll
            | Self::BeginCommit
            | Self::BeginDiscard
            | Self::RepositoryAction(_) => {
                context.capabilities.repository && !context.repository_pending
            }
            Self::SubmitReview => context.capabilities.submit,
            Self::CopyReview => context.capabilities.clipboard,
            Self::Focus(FocusPane::Files) | Self::ToggleFocus => context.navigation_available,
            _ => true,
        };
        capability
            && (context.document_ready
                || matches!(self, Self::SetScope(_) | Self::CycleScope | Self::Refresh))
    }
}
