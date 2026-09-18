use crate::{ClientError, DiffScope, DiffSnapshot, RemoteError, ReviewCapabilities};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, Default)]
pub enum ReconnectPolicy {
    Never,
    #[default]
    Retry,
}

#[derive(Debug, Clone, Default)]
pub struct ClientOptions {
    pub scope: DiffScope,
    pub reconnect: ReconnectPolicy,
}

impl From<DiffScope> for ClientOptions {
    fn from(scope: DiffScope) -> Self {
        Self {
            scope,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Failed(ClientError),
}

#[derive(Debug, Clone)]
pub struct ClientState {
    pub snapshot: Option<Arc<DiffSnapshot>>,
    pub connection: ConnectionState,
    pub capabilities: ReviewCapabilities,
    pub error: Option<RemoteError>,
}

impl ClientState {
    #[must_use]
    pub fn snapshot_if_changed(
        &self,
        installed: &mut Option<Arc<DiffSnapshot>>,
    ) -> Option<&DiffSnapshot> {
        let snapshot = self.snapshot.as_ref()?;
        if installed
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, snapshot))
        {
            return None;
        }
        *installed = Some(snapshot.clone());
        Some(snapshot)
    }

    #[must_use]
    pub fn status(&self) -> Option<String> {
        match &self.connection {
            ConnectionState::Connected => self.error.as_ref().map(ToString::to_string),
            ConnectionState::Connecting if self.snapshot.is_some() => {
                Some("Disconnected; reconnecting…".to_owned())
            }
            ConnectionState::Connecting => Some("Loading repository…".to_owned()),
            ConnectionState::Failed(error) => Some(error.to_string()),
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self.connection {
            ConnectionState::Connecting if self.snapshot.is_some() => "reconnecting",
            ConnectionState::Connecting => "loading",
            ConnectionState::Connected => "connected",
            ConnectionState::Failed(_) => "failed",
        }
    }
}

impl Default for ClientState {
    fn default() -> Self {
        Self {
            snapshot: None,
            connection: ConnectionState::Connecting,
            capabilities: crate::protocol::client::capabilities(false, false),
            error: None,
        }
    }
}
