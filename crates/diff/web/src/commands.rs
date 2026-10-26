use clankerdiff_core::{DiffReviewCommand, ReviewCapabilities};
use clankerdiff_markdown::MarkdownReviewCommand;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target", content = "command", rename_all = "snake_case")]
pub enum ViewerCommand {
    Diff(DiffReviewCommand),
    Markdown(MarkdownReviewCommand),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRequest {
    #[serde(default)]
    pub request_id: Option<u64>,
    #[serde(flatten)]
    pub command: ViewerCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandResult {
    pub request_id: Option<u64>,
    pub handled: bool,
    pub error: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("invalid review command JSON: {0}")]
    InvalidCommand(serde_json::Error),
    #[error("invalid review capabilities JSON: {0}")]
    InvalidCapabilities(serde_json::Error),
    #[error("the requested reviewer is not active")]
    InactiveReviewer,
}

pub const COMMAND_EVENT: &str = "diff-review-command";
pub const COMMAND_RESULT_EVENT: &str = "diff-review-command-result";
pub const CAPABILITIES_EVENT: &str = "diff-review-set-capabilities";

pub fn decode_command(json: &str) -> Result<CommandRequest, CommandError> {
    serde_json::from_str(json).map_err(CommandError::InvalidCommand)
}

pub fn decode_capabilities(json: &str) -> Result<ReviewCapabilities, CommandError> {
    serde_json::from_str(json).map_err(CommandError::InvalidCapabilities)
}
