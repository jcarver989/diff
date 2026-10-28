use crate::daemon::{DaemonError, DaemonResult, SpawnOptions, Timeouts};
use crate::diagnostics_store::{DiagnosticsStore, Freshness};
use crate::file_watcher::is_ignored_directory_name;
use crate::hooks::{TestHooks, TestPoint};
use crate::language_catalog::{DiagnosticsMode, ServerSpec};
use crate::language_server::{self, Command, Opened, ServerState, Shared};
use crate::protocol::{CallError, LSP_CONTENT_MODIFIED};
use crate::uri::{path_to_uri, uri_to_path};
use ignore::WalkBuilder;
use lsp_types::request::{DocumentDiagnosticRequest, Request};
use lsp_types::{
    DocumentDiagnosticReport, DocumentDiagnosticReportResult, PublishDiagnosticsParams, TextDocumentIdentifier, Uri,
};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use serde_json::{Value, json};
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::select;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{sleep, timeout};

const LSP_SERVER_CANCELLED: i32 = -32802;
const TRANSIENT_RETRY_LIMIT: u32 = 3;
const TRANSIENT_RETRY_DELAY: Duration = Duration::from_millis(500);

pub(crate) enum Refresh {
    Uri(Uri),
    Barrier(oneshot::Sender<()>),
}

#[derive(Clone)]
pub(crate) struct WorkspaceSession {
    commands: mpsc::Sender<Command>,
    state: watch::Sender<ServerState>,
    pid: Option<i32>,
    diagnostics: DiagnosticsStore,
    refresh: mpsc::UnboundedSender<Refresh>,
    mode: DiagnosticsMode,
    timeouts: Timeouts,
    hooks: TestHooks,
}

impl WorkspaceSession {
    pub(crate) fn spawn(root: &Path, server: &'static ServerSpec, options: &SpawnOptions) -> DaemonResult<Self> {
        let state =
            watch::Sender::new(ServerState { initialized: false, ready: !server.reports_server_status, alive: true });
        let diagnostics = DiagnosticsStore::new(options.timeouts.diagnostics_settle);
        let (refresh, refresh_rx) = mpsc::unbounded_channel();
        let shared = Shared {
            state: state.clone(),
            diagnostics: diagnostics.clone(),
            refresh: refresh.clone(),
            hooks: options.hooks.clone(),
        };
        let command = options.command.clone().unwrap_or_else(|| server.command());
        let spawned = language_server::spawn(root, server, &command, shared)?;
        let session = Self {
            commands: spawned.commands,
            state,
            pid: spawned.pid,
            diagnostics,
            refresh,
            mode: server.diagnostics_mode,
            timeouts: options.timeouts,
            hooks: options.hooks.clone(),
        };
        tokio::spawn(session.clone().run_refresh(root.to_path_buf(), server, refresh_rx));
        Ok(session)
    }

    pub(crate) async fn wait_until_initialized(&self) -> DaemonResult<()> {
        let mut state = self.state.subscribe();
        let initialized = state.wait_for(|state| state.initialized || !state.alive).await.is_ok_and(|s| s.initialized);
        if initialized { Ok(()) } else { Err(DaemonError::LspInitializeFailed) }
    }

    pub(crate) async fn wait_until_ready(&self, bound: Duration) {
        let mut state = self.state.subscribe();
        let ready = timeout(bound, state.wait_for(|state| state.ready || !state.alive));
        if self.hooks.observe_pending(TestPoint::ReadyWait, ready).await.is_err() {
            tracing::warn!("Language server still loading after {}s; sending requests anyway", bound.as_secs());
            self.state.send_modify(|state| state.ready = true);
        }
    }

    pub(crate) async fn request(&self, method: &str, params: &Value) -> Result<Value, CallError> {
        let Some(uri) = document_uri(method, params) else {
            return self.request_with_retry(method, params).await;
        };
        let _held = self.open(&uri).await;
        self.request_with_retry(method, params).await
    }

    pub(crate) async fn queue_diagnostic_refresh(&self, uri: Uri) {
        let _ = self.commands.send(Command::MarkWritten { uri: uri.clone() }).await;
        let _ = self.refresh.send(Refresh::Uri(uri));
    }

    pub(crate) async fn get_diagnostics(&self, uri: Option<&Uri>) -> Vec<PublishDiagnosticsParams> {
        match uri {
            Some(uri) if self.refresh_uri(uri).await == Freshness::Fresh => {}
            _ => self.barrier().await,
        }
        self.diagnostics.get(uri)
    }

    pub(crate) async fn shutdown(&self) {
        let _ = self.commands.send(Command::Shutdown).await;
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.state.borrow().alive
    }

    pub(crate) fn kill(&self) {
        if self.state.send_if_modified(|state| std::mem::replace(&mut state.alive, false))
            && let Some(pid) = self.pid
        {
            let _ = kill(Pid::from_raw(pid), Signal::SIGKILL);
        }
    }

    async fn barrier(&self) {
        let (done, finished) = oneshot::channel();
        if self.refresh.send(Refresh::Barrier(done)).is_ok() {
            let _ = timeout(self.timeouts.diagnostics, finished).await;
        }
    }

    async fn open(&self, uri: &Uri) -> HeldDocument {
        let content = tokio::fs::read_to_string(uri_to_path(uri)).await.ok();
        let Ok(permit) = self.commands.reserve().await else {
            return HeldDocument { opened: Opened::Missing, release: None };
        };
        let (reply, opened) = oneshot::channel();
        permit.send(Command::Open { uri: uri.clone(), content, reply });
        let mut held = HeldDocument { opened: Opened::Missing, release: Some((self.commands.clone(), uri.clone())) };
        held.opened = self
            .hooks
            .observe_pending(TestPoint::DocumentSyncWait(uri.clone()), opened)
            .await
            .unwrap_or(Opened::Missing);
        held
    }

    async fn refresh_uri(&self, uri: &Uri) -> Freshness {
        let held = self.open(uri).await;
        let freshness = match (self.mode, held.opened) {
            (_, Opened::Missing) | (DiagnosticsMode::Push, Opened::AlreadyOpen) => Freshness::Unconfirmed,
            (DiagnosticsMode::Push, Opened::Sent { version_before }) => {
                let fresh = self.diagnostics.wait_for_uri_fresh(uri, version_before, self.timeouts.diagnostics);
                self.hooks.observe_pending(TestPoint::DiagnosticsWait(uri.clone()), fresh).await
            }
            (DiagnosticsMode::Pull, _) => self.pull_diagnostics(uri).await,
        };
        drop(held);
        freshness
    }

    async fn pull_diagnostics(&self, uri: &Uri) -> Freshness {
        let params = json!({ "textDocument": TextDocumentIdentifier { uri: uri.clone() } });
        let request = self.request_with_retry(DocumentDiagnosticRequest::METHOD, &params);
        let report = timeout(self.timeouts.diagnostics, request)
            .await
            .ok()
            .and_then(Result::ok)
            .and_then(|value| serde_json::from_value(value).ok());
        let Some(DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report))) = report else {
            tracing::warn!(uri = uri.as_str(), "Language server returned no full diagnostic report");
            return Freshness::Unconfirmed;
        };
        self.diagnostics.publish(PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics: report.full_document_diagnostic_report.items,
            version: None,
        });
        Freshness::Fresh
    }

    async fn request_with_retry(&self, method: &str, params: &Value) -> Result<Value, CallError> {
        let mut retries = 0;
        loop {
            let (reply, response) = oneshot::channel();
            let command = Command::Request { method: method.to_owned(), params: params.clone(), reply };
            if self.commands.send(command).await.is_err() {
                return Err(CallError::Closed);
            }
            match response.await.unwrap_or(Err(CallError::Closed)) {
                Err(CallError::Lsp(err))
                    if matches!(err.code, LSP_CONTENT_MODIFIED | LSP_SERVER_CANCELLED)
                        && retries < TRANSIENT_RETRY_LIMIT =>
                {
                    retries += 1;
                    sleep(TRANSIENT_RETRY_DELAY).await;
                }
                result => return result,
            }
        }
    }

    async fn run_refresh(self, root: PathBuf, server: &'static ServerSpec, mut jobs: mpsc::UnboundedReceiver<Refresh>) {
        let discovered = tokio::task::spawn_blocking(move || discover(&root, server)).await.unwrap_or_default();
        let mut queue = Jobs::default();
        for uri in discovered {
            queue.push(Refresh::Uri(uri));
        }
        let mut state = self.state.subscribe();
        while self.is_alive() {
            while let Ok(job) = jobs.try_recv() {
                queue.push(job);
            }
            match queue.pop() {
                Some(Refresh::Uri(uri)) => {
                    self.refresh_uri(&uri).await;
                }
                Some(Refresh::Barrier(done)) => {
                    let _ = done.send(());
                }
                None => select! {
                    job = jobs.recv() => match job {
                        Some(job) => queue.push(job),
                        None => return,
                    },
                    _ = state.wait_for(|state| !state.alive) => return,
                },
            }
        }
    }
}

struct HeldDocument {
    opened: Opened,
    release: Option<(mpsc::Sender<Command>, Uri)>,
}

impl Drop for HeldDocument {
    fn drop(&mut self) {
        let Some((commands, uri)) = self.release.take() else {
            return;
        };
        if let Err(TrySendError::Full(close)) = commands.try_send(Command::Close { uri }) {
            tokio::spawn(async move {
                let _ = commands.send(close).await;
            });
        }
    }
}

#[derive(Default)]
struct Jobs {
    queue: VecDeque<Refresh>,
    queued: HashSet<Uri>,
}

impl Jobs {
    fn push(&mut self, job: Refresh) {
        if let Refresh::Uri(uri) = &job
            && !self.queued.insert(uri.clone())
        {
            return;
        }
        self.queue.push_back(job);
    }

    fn pop(&mut self) -> Option<Refresh> {
        let job = self.queue.pop_front()?;
        if let Refresh::Uri(uri) = &job {
            self.queued.remove(uri);
        }
        Some(job)
    }
}

fn discover(root: &Path, server: &ServerSpec) -> Vec<Uri> {
    WalkBuilder::new(root)
        .standard_filters(true)
        .filter_entry(|entry| entry.depth() == 0 || !is_ignored_directory_name(entry.file_name()))
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|file_type| file_type.is_file()) && server.serves(entry.path()))
        .filter_map(|entry| path_to_uri(entry.path()).ok())
        .collect()
}

fn document_uri(method: &str, params: &Value) -> Option<Uri> {
    if !method.starts_with("textDocument/") {
        return None;
    }
    params.pointer("/textDocument/uri").and_then(Value::as_str).and_then(|uri| uri.parse().ok())
}
