use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("update refers to a file the client does not hold, or repeats a path")]
    Files,
    #[error("message encoding: {0}")]
    Encoding(#[from] serde_json::Error),
    #[error("expected a text protocol message")]
    MessageType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteErrorCode {
    UnsupportedVersion,
    Protocol,
    Busy,
    Git,
    Watcher,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[error("{message}")]
pub struct RemoteError {
    pub code: RemoteErrorCode,
    pub message: String,
}

impl From<ProtocolError> for RemoteError {
    fn from(error: ProtocolError) -> Self {
        Self::new(RemoteErrorCode::Protocol, error.to_string())
    }
}

impl RemoteError {
    #[must_use]
    pub fn new(code: RemoteErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
