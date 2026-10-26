use crate::{MarkdownReviewDecision, MarkdownTargetId};
use clankerdiff_core::{CommandContext, InteractionPhase, ReviewCommand};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkdownFocusPane {
    #[default]
    Document,
    Outline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkdownReviewCommand {
    Review(ReviewCommand),
    Focus(MarkdownFocusPane),
    ToggleFocus,
    SelectTarget(MarkdownTargetId),
    SelectHeading(usize),
    MoveSelection(isize),
    Scroll {
        pane: MarkdownFocusPane,
        lines: isize,
    },
    Page(isize),
    First,
    Last,
    NextHeading,
    PreviousHeading,
    OpenSelected,
    Approve,
    RequestChanges,
    CopyReview(MarkdownReviewDecision),
}

impl From<ReviewCommand> for MarkdownReviewCommand {
    fn from(command: ReviewCommand) -> Self {
        Self::Review(command)
    }
}

impl MarkdownReviewCommand {
    #[must_use]
    pub fn enabled(self, context: &CommandContext) -> bool {
        if let Self::Review(command) = self {
            return command.enabled(context);
        }
        if context.phase != InteractionPhase::Browse || !context.document_ready {
            return false;
        }
        match self {
            Self::Focus(MarkdownFocusPane::Outline) | Self::ToggleFocus => {
                context.navigation_available
            }
            Self::Approve | Self::RequestChanges => context.capabilities.submit,
            Self::CopyReview(_) => context.capabilities.clipboard,
            _ => true,
        }
    }
}
