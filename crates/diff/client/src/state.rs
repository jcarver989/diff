use crate::{
    ClientError, DiffDocument, DiffScope, RemoteError, ReviewCapabilities, ReviewTarget,
    protocol::{client::capabilities, server::ServerEvent, shared::Event},
};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, Default)]
pub enum ReconnectPolicy {
    Never,
    #[default]
    Retry,
}

#[derive(Debug, Clone, Default)]
pub struct ClientOptions {
    pub target: ReviewTarget,
    pub reconnect: ReconnectPolicy,
}

impl From<ReviewTarget> for ClientOptions {
    fn from(target: ReviewTarget) -> Self {
        Self { target, ..Self::default() }
    }
}

impl From<DiffScope> for ClientOptions {
    fn from(scope: DiffScope) -> Self {
        ReviewTarget::from(scope).into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Failed(ClientError),
}

#[derive(Debug, Clone)]
pub struct ClientState {
    pub snapshot: Option<Arc<DiffDocument>>,
    pub connection: ConnectionState,
    pub capabilities: ReviewCapabilities,
    pub error: Option<RemoteError>,
}

impl ClientState {
    #[must_use]
    pub fn apply(self, event: &ServerEvent) -> Self {
        match event {
            Event::Initialize { .. } => Self { error: None, ..self }.with_capabilities(),
            Event::Document(snapshot) => {
                Self { snapshot: Some(snapshot.clone()), connection: ConnectionState::Connected, ..self }
                    .with_capabilities()
            }
            Event::Health { error } => Self { error: error.clone(), ..self },
            Event::Error(error) => {
                Self { connection: ConnectionState::Failed(ClientError::Remote(error.clone())), ..self }
                    .with_capabilities()
            }
            Event::RequestResult(_) | Event::Lookup { .. } => self,
        }
    }

    pub(crate) fn set_connection(&mut self, connection: ConnectionState) {
        self.connection = connection;
        self.refresh_capabilities();
    }

    fn with_capabilities(mut self) -> Self {
        self.refresh_capabilities();
        self
    }

    fn refresh_capabilities(&mut self) {
        let connected = matches!(self.connection, ConnectionState::Connected);
        self.capabilities = capabilities(connected, self.snapshot.as_deref());
    }

    #[must_use]
    pub fn snapshot_if_changed(&self, installed: &mut Option<Arc<DiffDocument>>) -> Option<&Arc<DiffDocument>> {
        let snapshot = self.snapshot.as_ref()?;
        if installed.as_ref().is_some_and(|old| Arc::ptr_eq(old, snapshot)) {
            return None;
        }
        *installed = Some(snapshot.clone());
        Some(snapshot)
    }

    #[must_use]
    pub fn status(&self) -> Option<String> {
        match &self.connection {
            ConnectionState::Connected => self.error.as_ref().map(ToString::to_string),
            ConnectionState::Connecting if self.snapshot.is_some() => Some("Disconnected; reconnecting…".to_owned()),
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
            capabilities: capabilities(false, None),
            error: None,
        }
    }
}

impl PartialEq for ClientState {
    fn eq(&self, other: &Self) -> bool {
        self.connection == other.connection
            && self.capabilities == other.capabilities
            && self.error == other.error
            && match (&self.snapshot, &other.snapshot) {
                (Some(snapshot), Some(other)) => Arc::ptr_eq(snapshot, other),
                (None, None) => true,
                _ => false,
            }
    }
}
