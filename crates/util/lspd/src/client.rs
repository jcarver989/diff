use crate::pending_requests::PendingRequests;
use crate::protocol::{
    CallError, DaemonCall, DaemonRequest, DaemonResponse, JsonFrames, LSP_CONTENT_MODIFIED, LspErrorResponse,
};
use crate::workspace_key::WorkspaceKey;
use futures::{SinkExt, StreamExt};
use lsp_types::request::{DocumentSymbolRequest, GotoDefinition, HoverRequest, References, Rename, Request};
use lsp_types::{
    DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams,
    Location, PartialResultParams, Position, PublishDiagnosticsParams, ReferenceContext, ReferenceParams, RenameParams,
    TextDocumentIdentifier, TextDocumentPositionParams, Uri, WorkDoneProgressParams, WorkspaceEdit,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::future::pending;
use std::io::{self, ErrorKind};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::UnixStream;
use tokio::process::Command;
use tokio::select;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::codec::{FramedRead, FramedWrite};

const MESSAGE_CAPACITY: usize = 64;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Failed to connect to daemon: {0}")]
    ConnectionFailed(#[source] io::Error),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Failed to spawn daemon: {0}")]
    SpawnFailed(#[source] io::Error),
    #[error("Timeout waiting for daemon to start")]
    SpawnTimeout,
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    #[error("Daemon disconnected")]
    Disconnected,
    #[error("Initialization failed: {0}")]
    InitializationFailed(String),
    #[error("LSP error (code={code}): {message}")]
    Lsp { code: i32, message: String },
    #[error("The document changed while the language server was answering; retry shortly")]
    ContentModified,
    #[error(
        "LSP request timed out after {after:?}; the language server was killed and will be replaced on the next request"
    )]
    TimedOut { after: Duration },
    #[error("The language server closed")]
    Closed,
    #[error("Daemon error: {0}")]
    Daemon(String),
}

type ClientResult<T> = Result<T, ClientError>;

#[derive(Clone)]
pub struct LspClient {
    messages: mpsc::Sender<Message>,
    connected: watch::Receiver<bool>,
}

impl LspClient {
    #[must_use]
    pub fn new(workspace: WorkspaceKey) -> Self {
        let (messages, message_rx) = mpsc::channel(MESSAGE_CAPACITY);
        let (connected_tx, connected) = watch::channel(false);
        let connection =
            Connection { workspace, link: None, pending: PendingRequests::starting_at(1), connected: connected_tx };
        tokio::spawn(connection.run(message_rx));
        Self { messages, connected }
    }

    pub async fn connect(&self) -> ClientResult<()> {
        let (reply, response) = oneshot::channel();
        self.messages.send(Message::Connect(reply)).await.map_err(|_| ClientError::Disconnected)?;
        response.await.map_err(|_| ClientError::Disconnected)?
    }

    #[must_use]
    pub fn is_connected(&self) -> bool {
        *self.connected.borrow()
    }

    pub async fn definition_locations(&self, uri: Uri, line: u32, character: u32) -> ClientResult<Vec<Location>> {
        let params = GotoDefinitionParams {
            text_document_position_params: position(uri, line, character),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        let response = self.call(GotoDefinition::METHOD, &params, || GotoDefinitionResponse::Array(vec![])).await?;
        Ok(match response {
            GotoDefinitionResponse::Scalar(location) => vec![location],
            GotoDefinitionResponse::Array(locations) => locations,
            GotoDefinitionResponse::Link(links) => links
                .into_iter()
                .map(|link| Location { uri: link.target_uri, range: link.target_selection_range })
                .collect(),
        })
    }

    pub async fn find_references(
        &self,
        uri: Uri,
        line: u32,
        character: u32,
        include_declaration: bool,
    ) -> ClientResult<Vec<Location>> {
        let params = ReferenceParams {
            text_document_position: position(uri, line, character),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: ReferenceContext { include_declaration },
        };
        self.call(References::METHOD, &params, Vec::new).await
    }

    pub async fn hover(&self, uri: Uri, line: u32, character: u32) -> ClientResult<Option<Hover>> {
        let params = HoverParams {
            text_document_position_params: position(uri, line, character),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        self.call(HoverRequest::METHOD, &params, || None).await
    }

    pub async fn document_symbol(&self, uri: Uri) -> ClientResult<DocumentSymbolResponse> {
        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier::new(uri),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        self.call(DocumentSymbolRequest::METHOD, &params, || DocumentSymbolResponse::Flat(vec![])).await
    }

    pub async fn rename(
        &self,
        uri: Uri,
        line: u32,
        character: u32,
        new_name: String,
    ) -> ClientResult<Option<WorkspaceEdit>> {
        let params = RenameParams {
            text_document_position: position(uri, line, character),
            new_name,
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        self.call(Rename::METHOD, &params, || None).await
    }

    pub async fn get_diagnostics(&self, uri: Option<Uri>) -> ClientResult<Vec<PublishDiagnosticsParams>> {
        self.request(DaemonCall::Diagnostics { uri })
            .await
            .and_then(|value| serde_json::from_value(value).map_err(|err| ClientError::ProtocolError(err.to_string())))
    }

    pub async fn queue_diagnostic_refresh(&self, uri: Uri) -> ClientResult<()> {
        self.request(DaemonCall::QueueRefresh { uri }).await.map(|_| ())
    }

    pub async fn call<T: Serialize, U: DeserializeOwned>(
        &self,
        method: &str,
        params: &T,
        default: impl FnOnce() -> U,
    ) -> ClientResult<U> {
        let params = serde_json::to_value(params).map_err(|err| ClientError::ProtocolError(err.to_string()))?;
        let value = self.request(DaemonCall::Lsp { method: method.to_string(), params }).await?;

        if value.is_null() {
            Ok(default())
        } else {
            serde_json::from_value(value).map_err(|err| ClientError::ProtocolError(format!("Parse error: {err}")))
        }
    }

    async fn request(&self, call: DaemonCall) -> ClientResult<Value> {
        let (reply, response) = oneshot::channel();
        self.messages.send(Message::Call { call, reply }).await.map_err(|_| ClientError::Disconnected)?;
        response.await.map_err(|_| ClientError::Disconnected)?
    }
}

impl From<CallError> for ClientError {
    fn from(error: CallError) -> Self {
        match error {
            CallError::Lsp(error) if error.code == LSP_CONTENT_MODIFIED => Self::ContentModified,
            CallError::Lsp(LspErrorResponse { code, message }) => Self::Lsp { code, message },
            CallError::TimedOut { after } => Self::TimedOut { after },
            CallError::Closed => Self::Closed,
            CallError::Daemon(message) => Self::Daemon(message),
        }
    }
}

enum Message {
    Connect(oneshot::Sender<ClientResult<()>>),
    Call { call: DaemonCall, reply: oneshot::Sender<ClientResult<Value>> },
}

struct Link {
    reader: FramedRead<ReadHalf<UnixStream>, JsonFrames<DaemonResponse>>,
    writer: FramedWrite<WriteHalf<UnixStream>, JsonFrames<DaemonRequest>>,
}

struct Connection {
    workspace: WorkspaceKey,
    link: Option<Link>,
    pending: PendingRequests<ClientResult<Value>>,
    connected: watch::Sender<bool>,
}

impl Connection {
    async fn run(mut self, mut messages: mpsc::Receiver<Message>) {
        loop {
            select! {
                message = messages.recv() => match message {
                    Some(Message::Connect(reply)) => {
                        if !reply.is_closed() {
                            let _ = reply.send(self.ensure_link().await);
                        }
                    }
                    Some(Message::Call { call, reply }) => self.send(call, reply).await,
                    None => break,
                },
                response = next_response(self.link.as_mut()) => match response {
                    Some(DaemonResponse::Reply { id, result }) => self.pending.resolve(id, result.map_err(ClientError::from)),
                    Some(DaemonResponse::Initialized(_)) => {}
                    None => self.disconnect(),
                },
                id = self.pending.abandoned() => self.cancel(id).await,
            }
        }
        self.disconnect();
    }

    async fn send(&mut self, call: DaemonCall, reply: oneshot::Sender<ClientResult<Value>>) {
        if reply.is_closed() {
            return;
        }
        if let Err(error) = self.ensure_link().await {
            let _ = reply.send(Err(error));
            return;
        }
        let id = self.pending.insert(reply);
        let Some(link) = self.link.as_mut() else { return };
        if let Err(error) = link.writer.send(DaemonRequest::Call { id, call }).await {
            self.pending.resolve(id, Err(ClientError::Io(error)));
            self.disconnect();
        }
    }

    async fn cancel(&mut self, id: i64) {
        if let Some(link) = self.link.as_mut()
            && link.writer.send(DaemonRequest::Cancel { id }).await.is_err()
        {
            self.disconnect();
        }
    }

    async fn ensure_link(&mut self) -> ClientResult<()> {
        if self.link.is_none() {
            self.link = Some(open(&self.workspace).await?);
            self.connected.send_replace(true);
        }
        Ok(())
    }

    fn disconnect(&mut self) {
        self.link = None;
        self.connected.send_replace(false);
        self.pending.fail_all(|| Err(ClientError::Disconnected));
    }
}

async fn next_response(link: Option<&mut Link>) -> Option<DaemonResponse> {
    let Some(link) = link else { return pending().await };
    match link.reader.next().await? {
        Ok(response) => Some(response),
        Err(error) => {
            tracing::debug!(%error, "Error reading daemon response");
            None
        }
    }
}

async fn open(workspace: &WorkspaceKey) -> ClientResult<Link> {
    let socket = workspace.socket_path();
    let stream = match UnixStream::connect(&socket).await {
        Ok(stream) => stream,
        Err(err) if matches!(err.kind(), ErrorKind::ConnectionRefused | ErrorKind::NotFound) => {
            spawn_daemon(&socket).await?;
            UnixStream::connect(&socket).await.map_err(ClientError::ConnectionFailed)?
        }
        Err(err) => return Err(ClientError::ConnectionFailed(err)),
    };
    let (reader, writer) = tokio::io::split(stream);
    let mut link = Link {
        reader: FramedRead::new(reader, JsonFrames::default()),
        writer: FramedWrite::new(writer, JsonFrames::default()),
    };
    let initialize =
        DaemonRequest::Initialize { root: workspace.root().to_path_buf(), server: workspace.server().name.to_owned() };
    link.writer.send(initialize).await?;
    match link.reader.next().await.transpose()? {
        Some(DaemonResponse::Initialized(result)) => result.map_err(ClientError::InitializationFailed)?,
        Some(DaemonResponse::Reply { .. }) => {
            return Err(ClientError::ProtocolError("Unexpected response to Initialize".into()));
        }
        None => return Err(ClientError::ProtocolError("Connection closed during initialization".into())),
    }
    Ok(link)
}

async fn spawn_daemon(socket_path: &Path) -> ClientResult<()> {
    let binary = std::env::current_exe().map_err(ClientError::SpawnFailed)?;
    let log_file = socket_path.with_extension("log");

    let mut cmd = Command::new(&binary);
    cmd.arg("lspd")
        .arg("--socket")
        .arg(socket_path)
        .arg("--log-file")
        .arg(&log_file)
        .arg("--log-level")
        .arg("debug")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let mut child = cmd.spawn().map_err(ClientError::SpawnFailed)?;

    for _ in 0..50 {
        match child.try_wait() {
            Ok(Some(status)) if !status.success() => {
                return Err(ClientError::SpawnFailed(io::Error::other(format!("Daemon exited with status: {status}"))));
            }
            Ok(_) => {}
            Err(err) => return Err(ClientError::SpawnFailed(err)),
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
        if UnixStream::connect(socket_path).await.is_ok() {
            tokio::spawn(async move {
                match child.wait().await {
                    Ok(status) => tracing::debug!(%status, "lspd launcher reaped"),
                    Err(err) => tracing::warn!(%err, "Failed to reap lspd launcher"),
                }
            });
            return Ok(());
        }
    }

    let _ = child.kill().await;
    let _ = child.wait().await;
    Err(ClientError::SpawnTimeout)
}

fn position(uri: Uri, line: u32, character: u32) -> TextDocumentPositionParams {
    TextDocumentPositionParams::new(TextDocumentIdentifier::new(uri), Position::new(line, character))
}
