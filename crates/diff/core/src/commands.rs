use crate::{DiffScope, DiffSide, RepositoryAction, RevealAmount, TargetLocation, ViewMode};
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
    CommitPicker,
    RepositoryPrompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[expect(clippy::struct_excessive_bools, reason = "Capabilities are independent flags")]
pub struct ReviewCapabilities {
    pub repository: bool,
    pub refresh: bool,
    pub scope: bool,
    pub submit: bool,
    pub clipboard: bool,
    #[serde(default)]
    pub lookups: bool,
}

impl ReviewCapabilities {
    pub const NONE: Self =
        Self { repository: false, refresh: false, scope: false, submit: false, clipboard: false, lookups: false };
}

impl Default for ReviewCapabilities {
    fn default() -> Self {
        Self { repository: true, refresh: true, scope: true, submit: true, clipboard: true, lookups: false }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "Command conditions are independent flags")]
pub struct CommandContext {
    pub phase: InteractionPhase,
    pub capabilities: ReviewCapabilities,
    pub repository_pending: bool,
    pub document_ready: bool,
    pub source_view: bool,
    pub navigation_available: bool,
    pub themes_available: bool,
    pub commits_available: bool,
}

impl Default for CommandContext {
    fn default() -> Self {
        Self {
            phase: InteractionPhase::Browse,
            capabilities: ReviewCapabilities::default(),
            repository_pending: false,
            document_ready: true,
            source_view: false,
            navigation_available: true,
            themes_available: false,
            commits_available: false,
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
            Self::SelectTheme(_) | Self::MoveTheme(_) | Self::CommitTheme => phase == InteractionPhase::ThemePicker,
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
    ToggleFileCollapsed(Option<usize>),
    ToggleAllFilesCollapsed,
    ToggleSourceView,
    SetViewMode(ViewMode),
    ToggleLayout,
    SubmitReview,
    CopyReview,
    SetScope(DiffScope),
    CycleScope,
    StepTarget(isize),
    GoToPage(usize),
    OpenCommitPicker,
    MoveCommitPicker(isize),
    Refresh,
    ToggleStage,
    StageAll,
    UnstageAll,
    BeginCommit,
    BeginDiscard,
    RepositoryAction(RepositoryAction),
    ShowDefinition,
    JumpTo(TargetLocation),
    NavigateBack,
    NavigateForward,
}

impl From<ReviewCommand> for DiffReviewCommand {
    fn from(command: ReviewCommand) -> Self {
        Self::Review(command)
    }
}

impl DiffReviewCommand {
    #[must_use]
    pub fn enabled(&self, context: &CommandContext) -> bool {
        if context.source_view
            && matches!(
                self,
                Self::RevealGap(_)
                    | Self::ToggleFullFile
                    | Self::ToggleFileCollapsed(_)
                    | Self::ToggleAllFilesCollapsed
                    | Self::SetViewMode(_)
                    | Self::ToggleLayout
                    | Self::SelectSide(_)
                    | Self::Review(
                        ReviewCommand::BeginComment
                            | ReviewCommand::EditComment
                            | ReviewCommand::DeleteComment
                            | ReviewCommand::UndoComment
                    )
            )
        {
            return false;
        }
        if let Self::Review(command) = self {
            return command.enabled(context);
        }
        let paging = context.capabilities.scope && !context.repository_pending;
        if context.phase == InteractionPhase::CommitPicker {
            return match self {
                Self::MoveCommitPicker(_) => true,
                Self::GoToPage(_) => paging,
                _ => false,
            };
        }
        if context.phase != InteractionPhase::Browse {
            return false;
        }
        let capability = match self {
            Self::SetScope(_) | Self::CycleScope | Self::StepTarget(_) | Self::GoToPage(_) => paging,
            Self::OpenCommitPicker => paging && context.commits_available,
            Self::MoveCommitPicker(_) => false,
            Self::Refresh => context.capabilities.refresh && !context.repository_pending,
            Self::ToggleStage
            | Self::StageAll
            | Self::UnstageAll
            | Self::BeginCommit
            | Self::BeginDiscard
            | Self::RepositoryAction(_) => context.capabilities.repository && !context.repository_pending,
            Self::SubmitReview => context.capabilities.submit,
            Self::CopyReview => context.capabilities.clipboard,
            Self::Focus(FocusPane::Files) | Self::ToggleFocus => context.navigation_available,
            _ => true,
        };
        capability
            && (context.document_ready
                || matches!(
                    self,
                    Self::SetScope(_)
                        | Self::CycleScope
                        | Self::StepTarget(_)
                        | Self::GoToPage(_)
                        | Self::OpenCommitPicker
                        | Self::Refresh
                ))
    }
}
