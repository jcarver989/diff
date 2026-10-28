use crate::daemon::{DaemonError, DaemonResult};
use crate::diagnostics_store::DiagnosticsStore;
use crate::file_watcher::{WatchBatch, WatchedFiles, elapsed};
use crate::hooks::{TestHooks, TestPoint};
use crate::language_catalog::{DiagnosticsMode, LanguageId, ServerCommand, ServerSpec};
use crate::pending_requests::PendingRequests;
use crate::protocol::{CallError, LspErrorResponse};
use crate::uri::{path_to_uri, uri_to_path};
use crate::workspace_session::Refresh;
use futures::{SinkExt, StreamExt};
use lsp_types::notification::{
    Cancel, DidChangeWatchedFiles, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument, Initialized,
    Notification, PublishDiagnostics,
};
use lsp_types::request::{
    Initialize, RegisterCapability, Request, UnregisterCapability, WorkDoneProgressCreate, WorkspaceDiagnosticRefresh,
};
use lsp_types::{
    DidChangeWatchedFilesParams, DidChangeWatchedFilesRegistrationOptions, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, RegistrationParams, TextDocumentIdentifier, TextDocumentItem,
    UnregistrationParams, Uri,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::{Child, ChildStdin, ChildStdout, Command as Process};
use tokio::select;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::bytes::{Buf, BytesMut};
use tokio_util::codec::{Decoder, Encoder, FramedRead, FramedWrite};

const INITIALIZE_REQUEST_ID: i64 = 1;
const SERVER_STATUS_METHOD: &str = "experimental/serverStatus";
const REQUEST_RECEIVED_METHOD: &str = "lspd/testing/requestReceived";

#[derive(Clone, Copy, Debug)]
pub(crate) struct ServerState {
    pub(crate) initialized: bool,
    pub(crate) ready: bool,
    pub(crate) alive: bool,
}

pub(crate) enum Command {
    Request { method: String, params: Value, reply: oneshot::Sender<Result<Value, CallError>> },
    Open { uri: Uri, content: Option<String>, reply: oneshot::Sender<Opened> },
    Close { uri: Uri },
    MarkWritten { uri: Uri },
    Shutdown,
}

#[derive(Clone, Copy)]
pub(crate) enum Opened {
    Sent { version_before: u64 },
    AlreadyOpen,
    Missing,
}

pub(crate) struct Spawned {
    pub(crate) commands: mpsc::Sender<Command>,
    pub(crate) pid: Option<i32>,
}

pub(crate) struct Shared {
    pub(crate) state: watch::Sender<ServerState>,
    pub(crate) diagnostics: DiagnosticsStore,
    pub(crate) refresh: mpsc::UnboundedSender<Refresh>,
    pub(crate) hooks: TestHooks,
}

#[derive(Default)]
struct Document {
    holders: usize,
    content_hash: Option<u64>,
    needs_refresh: bool,
    needs_save: bool,
}

struct LanguageServer {
    process: Child,
    reader: FramedRead<ChildStdout, LspCodec>,
    writer: FramedWrite<ChildStdin, LspCodec>,
    commands: mpsc::Receiver<Command>,
    pending: PendingRequests<Result<Value, CallError>>,
    documents: HashMap<Uri, Document>,
    watched: WatchedFiles,
    server: &'static ServerSpec,
    shared: Shared,
}

pub(crate) fn spawn(
    root: &Path,
    server: &'static ServerSpec,
    command: &ServerCommand,
    shared: Shared,
) -> DaemonResult<Spawned> {
    let program = resolve_command(root, &command.command);
    let mut process = Process::new(&program)
        .current_dir(root)
        .args(&command.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| DaemonError::LspSpawnFailed(format!("{}: {e}", program.display())))?;
    let missing = |pipe: &str| DaemonError::LspSpawnFailed(format!("Failed to capture {pipe}"));
    let stdin = process.stdin.take().ok_or_else(|| missing("stdin"))?;
    let stdout = process.stdout.take().ok_or_else(|| missing("stdout"))?;
    let pid = process.id().and_then(|id| i32::try_from(id).ok());
    let (commands, command_rx) = mpsc::channel(100);
    let actor = LanguageServer {
        process,
        reader: FramedRead::new(stdout, LspCodec),
        writer: FramedWrite::new(stdin, LspCodec),
        commands: command_rx,
        pending: PendingRequests::starting_at(INITIALIZE_REQUEST_ID + 1),
        documents: HashMap::new(),
        watched: WatchedFiles::new(root),
        server,
        shared,
    };
    tokio::spawn(actor.run(root.to_path_buf()));
    Ok(Spawned { commands, pid })
}

impl LanguageServer {
    async fn run(mut self, root: PathBuf) {
        match self.initialize(&root).await {
            Ok(()) => {
                self.shared.state.send_modify(|state| state.initialized = true);
                self.serve().await;
            }
            Err(err) => tracing::error!(%err, "Failed to initialize LSP transport"),
        }
        self.shared.state.send_modify(|state| state.alive = false);
        self.pending.fail_all(|| Err(CallError::Closed));
    }

    async fn serve(&mut self) {
        loop {
            select! {
                message = self.reader.next() => match message {
                    Some(Ok(message)) => self.receive(message).await,
                    Some(Err(err)) => tracing::warn!(%err, "Error reading LSP message"),
                    None => return,
                },
                command = self.commands.recv() => match command {
                    Some(Command::Shutdown) | None => return,
                    Some(command) => self.handle(command).await,
                },
                Some(event) = self.watched.events.recv() => self.watched.accumulate(&event),
                () = elapsed(&mut self.watched.debounce) => {
                    let batch = self.watched.take();
                    self.forward(batch).await;
                }
                id = self.pending.abandoned() => self.cancel(id).await,
                _ = self.process.wait() => return,
            }
        }
    }

    async fn handle(&mut self, command: Command) {
        match command {
            Command::Request { method, params, reply } => {
                let id = self.pending.insert(reply);
                if let Err(err) = self.send(json!({ "id": id, "method": method, "params": params })).await {
                    tracing::warn!(%err, "Failed to write LSP request");
                    self.pending.resolve(id, Err(CallError::Closed));
                }
            }
            Command::Open { uri, content, reply } => {
                let opened = self.open(uri, content).await;
                let _ = reply.send(opened);
            }
            Command::Close { uri } => self.close(uri).await,
            Command::MarkWritten { uri } => self.documents.entry(uri).or_default().needs_save = true,
            Command::Shutdown => {}
        }
    }

    async fn open(&mut self, uri: Uri, content: Option<String>) -> Opened {
        let Some(content) = content else {
            self.documents.remove(&uri);
            self.shared.diagnostics.forget_uri(&uri);
            return Opened::Missing;
        };
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        let content_hash = Some(hasher.finish());
        let document = self.documents.entry(uri.clone()).or_default();
        document.holders += 1;
        let mut notifications = Vec::new();
        if document.holders > 1 && document.content_hash != content_hash {
            notifications.push(notification::<DidCloseTextDocument>(&DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier::new(uri.clone()),
            }));
        }
        if document.holders == 1 || document.content_hash != content_hash {
            let language = LanguageId::from_path(&uri_to_path(&uri)).map_or("plaintext", LanguageId::as_str).to_owned();
            notifications.push(notification::<DidOpenTextDocument>(&DidOpenTextDocumentParams {
                text_document: TextDocumentItem::new(uri.clone(), language, 1, content),
            }));
        }
        document.content_hash = content_hash;
        if std::mem::take(&mut document.needs_save) {
            notifications.push(notification::<DidSaveTextDocument>(&DidSaveTextDocumentParams {
                text_document: TextDocumentIdentifier::new(uri.clone()),
                text: None,
            }));
        }
        if notifications.is_empty() {
            return Opened::AlreadyOpen;
        }
        self.shared.hooks.pause(TestPoint::DocumentOpen(uri.clone())).await;
        let version_before = self.shared.diagnostics.current_uri_version(&uri);
        for notification in notifications {
            self.notify(notification).await;
        }
        Opened::Sent { version_before }
    }

    async fn close(&mut self, uri: Uri) {
        let Some(document) = self.documents.get_mut(&uri).filter(|document| document.holders > 0) else {
            return;
        };
        document.holders -= 1;
        if document.holders > 0 {
            return;
        }
        let refresh = std::mem::take(&mut document.needs_refresh);
        self.notify(notification::<DidCloseTextDocument>(&DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
        }))
        .await;
        if refresh {
            self.refresh(vec![uri]);
        }
    }

    async fn forward(&mut self, batch: WatchBatch) {
        let changes: Vec<_> = batch
            .changes
            .into_iter()
            .filter(|change| match self.documents.get_mut(&change.uri) {
                Some(document) if document.holders > 0 => {
                    document.needs_refresh = true;
                    false
                }
                _ => true,
            })
            .collect();
        self.refresh(changes.iter().map(|change| change.uri.clone()).chain(batch.discovered).collect());
        if !changes.is_empty() {
            self.notify(notification::<DidChangeWatchedFiles>(&DidChangeWatchedFilesParams { changes })).await;
        }
    }

    fn refresh(&self, uris: Vec<Uri>) {
        for uri in uris.into_iter().filter(|uri| self.server.serves(&uri_to_path(uri))) {
            let _ = self.shared.refresh.send(Refresh::Uri(uri));
        }
    }

    async fn initialize(&mut self, root: &Path) -> io::Result<()> {
        let root_uri = path_to_uri(root).map_err(|err| io::Error::new(ErrorKind::InvalidInput, err))?;
        let pull = self.server.diagnostics_mode == DiagnosticsMode::Pull;
        let name = root.file_name().and_then(|name| name.to_str()).unwrap_or("workspace");
        let mut workspace = json!({
            "didChangeWatchedFiles": { "dynamicRegistration": true, "relativePatternSupport": false },
        });
        let mut text_document = json!({
            "publishDiagnostics": { "relatedInformation": true },
            "definition": { "dynamicRegistration": false, "linkSupport": true },
            "references": { "dynamicRegistration": false },
            "hover": { "dynamicRegistration": false, "contentFormat": ["markdown", "plaintext"] },
            "documentSymbol": { "hierarchicalDocumentSymbolSupport": true },
        });
        if pull {
            text_document["diagnostic"] = json!({ "dynamicRegistration": false, "relatedDocumentSupport": false });
            workspace["diagnostics"] = json!({ "refreshSupport": true });
        }
        let params = json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "workspaceFolders": [{ "uri": root_uri, "name": name }],
            "capabilities": {
                "general": {},
                "textDocument": text_document,
                "workspace": workspace,
                "experimental": { "serverStatusNotification": true },
            },
        });
        self.send(json!({ "id": INITIALIZE_REQUEST_ID, "method": Initialize::METHOD, "params": params })).await?;
        while let Some(message) = self.reader.next().await {
            let message = match message {
                Ok(message) => message,
                Err(err) => {
                    tracing::warn!(%err, "Error reading LSP message");
                    continue;
                }
            };
            if message.id.as_ref().and_then(Value::as_i64) == Some(INITIALIZE_REQUEST_ID) {
                if let Some(error) = message.error {
                    return Err(io::Error::other(format!(
                        "server rejected `initialize`: {} (code {})",
                        error.message, error.code
                    )));
                }
                return self.send(json!({ "method": Initialized::METHOD, "params": {} })).await;
            }
            self.receive(message).await;
        }
        Err(io::Error::new(ErrorKind::UnexpectedEof, "LSP closed during initialization"))
    }

    async fn receive(&mut self, message: Incoming) {
        match (message.id, message.method) {
            (Some(id), Some(method)) => {
                let response = match method.as_str() {
                    RegisterCapability::METHOD | UnregisterCapability::METHOD => {
                        self.watch(&method, message.params);
                        json!({ "id": id, "result": null })
                    }
                    WorkDoneProgressCreate::METHOD => json!({ "id": id, "result": null }),
                    WorkspaceDiagnosticRefresh::METHOD => {
                        self.refresh(self.shared.diagnostics.uris());
                        json!({ "id": id, "result": null })
                    }
                    _ => {
                        json!({ "id": id, "error": { "code": -32601, "message": format!("Method not found: {method}") } })
                    }
                };
                let _ = self.send(response).await;
            }
            (Some(id), None) => {
                if let Some(id) = id.as_i64() {
                    self.pending.resolve(id, message.error.map_or(Ok(message.result), |err| Err(CallError::Lsp(err))));
                }
            }
            (None, Some(method)) => match method.as_str() {
                PublishDiagnostics::METHOD => {
                    if let Ok(diagnostics) = serde_json::from_value(message.params) {
                        self.shared.diagnostics.publish(diagnostics);
                    }
                }
                SERVER_STATUS_METHOD => {
                    if let Some(quiescent) = message.params.get("quiescent").and_then(Value::as_bool) {
                        self.shared.state.send_modify(|state| state.ready = quiescent);
                    }
                }
                REQUEST_RECEIVED_METHOD => {
                    if let Some(method) = message.params.get("method").and_then(Value::as_str) {
                        self.shared.hooks.pause(TestPoint::RequestReceived(method.to_owned())).await;
                    }
                }
                _ => {}
            },
            (None, None) => {}
        }
    }

    fn watch(&mut self, method: &str, params: Value) {
        if method == RegisterCapability::METHOD {
            let registrations = serde_json::from_value::<RegistrationParams>(params).map(|p| p.registrations);
            for registration in registrations.unwrap_or_default() {
                if registration.method == DidChangeWatchedFiles::METHOD
                    && let Some(options) = registration.register_options
                    && let Ok(options) = serde_json::from_value::<DidChangeWatchedFilesRegistrationOptions>(options)
                {
                    self.watched.register(registration.id, options.watchers);
                }
            }
        } else if let Ok(params) = serde_json::from_value::<UnregistrationParams>(params) {
            for unregistration in params.unregisterations {
                if unregistration.method == DidChangeWatchedFiles::METHOD {
                    self.watched.unregister(&unregistration.id);
                }
            }
        }
    }

    async fn notify(&mut self, notification: Value) {
        if let Err(err) = self.send(notification).await {
            tracing::warn!(%err, "Failed to forward LSP notification");
        }
    }

    async fn cancel(&mut self, id: i64) {
        if let Err(err) = self.send(json!({ "method": Cancel::METHOD, "params": { "id": id } })).await {
            tracing::warn!(%err, "Failed to cancel LSP request");
        }
    }

    async fn send(&mut self, mut message: Value) -> io::Result<()> {
        message["jsonrpc"] = json!("2.0");
        self.writer.send(message).await
    }
}

fn notification<N: Notification>(params: &N::Params) -> Value {
    json!({ "method": N::METHOD, "params": params })
}

fn resolve_command(root: &Path, command: &str) -> PathBuf {
    let command = Path::new(command);
    if command.is_absolute() || command.components().count() > 1 {
        return command.to_path_buf();
    }
    let local = root.join("node_modules").join(".bin").join(command);
    if local.exists() { local } else { command.to_path_buf() }
}

#[derive(Deserialize)]
struct Incoming {
    id: Option<Value>,
    method: Option<String>,
    #[serde(default)]
    params: Value,
    #[serde(default)]
    result: Value,
    error: Option<LspErrorResponse>,
}

struct LspCodec;

impl Decoder for LspCodec {
    type Item = Incoming;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<Incoming>> {
        loop {
            let Some(header_len) = src.windows(4).position(|window| window == b"\r\n\r\n") else {
                return Ok(None);
            };
            let body_start = header_len + 4;
            let content_length = String::from_utf8_lossy(&src[..header_len])
                .lines()
                .find_map(|line| line.strip_prefix("Content-Length: ")?.trim().parse::<usize>().ok());
            let Some(content_length) = content_length else {
                tracing::warn!("Skipping LSP message without a Content-Length header");
                src.advance(body_start);
                continue;
            };
            if src.len() < body_start + content_length {
                src.reserve(body_start + content_length - src.len());
                return Ok(None);
            }
            let frame = src.split_to(body_start + content_length);
            match serde_json::from_slice(&frame[body_start..]) {
                Ok(message) => return Ok(Some(message)),
                Err(err) => tracing::warn!(%err, "Skipping invalid LSP message"),
            }
        }
    }
}

impl Encoder<Value> for LspCodec {
    type Error = io::Error;

    fn encode(&mut self, message: Value, dst: &mut BytesMut) -> io::Result<()> {
        let body = serde_json::to_vec(&message)?;
        dst.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        dst.extend_from_slice(&body);
        Ok(())
    }
}
