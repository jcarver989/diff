use crate::protocol::shared::{ProtocolError, RemoteError};
use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum ClientError {
    #[error("connection closed")]
    Disconnected,
    #[error("command may have completed before the connection was lost; verify before retrying")]
    OutcomeUnknown,
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error(transparent)]
    Remote(#[from] RemoteError),
}

impl From<ProtocolError> for ClientError {
    fn from(error: ProtocolError) -> Self {
        Self::Protocol(error.to_string())
    }
}

#[cfg(feature = "websocket")]
pub(crate) fn transport(error: impl ToString) -> ClientError {
    let message = error.to_string();
    drop(error);
    ClientError::Transport(message)
}
