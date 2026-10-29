use crate::hooks::TestHooks;
use crate::language_catalog::ServerCommand;
use crate::protocol::{DaemonCall, DaemonRequest, DaemonResponse, JsonFrames};
use crate::workspace::{SessionRequest, Workspace};
use crate::workspace_key::WorkspaceKey;
use crate::workspace_session::WorkspaceSession;
use futures::future::join_all;
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::fs::{File, create_dir_all, remove_file};
use std::io;
use std::os::unix::net::UnixListener as StdUnixListener;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::time::Duration;
use thiserror::Error;
use tokio::net::{UnixListener, UnixStream};
use tokio::select;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::{Instant, interval_at, sleep_until};
use tokio_util::codec::{FramedRead, FramedWrite};

const LIVENESS_INTERVAL: Duration = Duration::from_secs(10);
const SESSION_CAPACITY: usize = 16;

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Failed to bind to socket: {0}")]
    BindFailed(#[source] io::Error),
    #[error("Failed to spawn LSP: {0}")]
    LspSpawnFailed(String),
    #[error("Language server failed to complete initialization (it exited or rejected `initialize`)")]
    LspInitializeFailed,
    #[error("Language server did not answer `initialize` within {}s", .0.as_secs())]
    LspInitializeTimedOut(Duration),
    #[error("Lockfile error: {0}")]
    LockfileError(#[source] io::Error),
    #[error("The daemon is shutting down")]
    ShuttingDown,
}

pub(crate) type DaemonResult<T> = Result<T, DaemonError>;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Timeouts {
    pub(crate) request: Duration,
    pub(crate) diagnostics: Duration,
    pub(crate) diagnostics_settle: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            request: Duration::from_secs(120),
            diagnostics: Duration::from_secs(20),
            diagnostics_settle: Duration::from_millis(600),
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct SpawnOptions {
    pub(crate) timeouts: Timeouts,
    pub(crate) command: Option<ServerCommand>,
    pub(crate) hooks: TestHooks,
}

pub(crate) struct LspDaemon {
    socket_path: PathBuf,
    listener: StdUnixListener,
    _lock: File,
    idle_timeout: Option<Duration>,
    options: SpawnOptions,
}

impl LspDaemon {
    pub(crate) fn bind(
        socket_path: PathBuf,
        idle_timeout: Option<Duration>,
        options: SpawnOptions,
    ) -> DaemonResult<Self> {
        if let Some(parent) = socket_path.parent() {
            create_dir_all(parent)?;
        }
        let lock = File::create(socket_path.with_extension("lock")).map_err(DaemonError::LockfileError)?;
        lock.try_lock().map_err(|err| DaemonError::LockfileError(err.into()))?;
        let _ = remove_file(&socket_path);
        let listener = StdUnixListener::bind(&socket_path).map_err(DaemonError::BindFailed)?;
        listener.set_nonblocking(true).map_err(DaemonError::BindFailed)?;
        tracing::info!("Daemon listening on {:?}", socket_path);
        Ok(Self { socket_path, listener, _lock: lock, idle_timeout, options })
    }

    pub(crate) async fn run_until(self, shutdown: impl Future<Output = ()>) -> DaemonResult<()> {
        let listener = UnixListener::from_std(self.listener).map_err(DaemonError::BindFailed)?;
        let mut shutdown = pin!(shutdown);
        let mut clients = JoinSet::new();
        let mut sessions = HashMap::new();
        let (requests, mut session_requests) = mpsc::channel::<SessionRequest>(SESSION_CAPACITY);
        let mut last_activity = Instant::now();
        let mut liveness = interval_at(Instant::now() + LIVENESS_INTERVAL, LIVENESS_INTERVAL);
        let mut next_client = 0_u64;
        loop {
            let idle = self.idle_timeout.filter(|_| clients.is_empty());
            select! {
                biased;
                () = &mut shutdown => break,
                accepted = listener.accept() => match accepted {
                    Ok((stream, _)) => {
                        last_activity = Instant::now();
                        next_client += 1;
                        clients.spawn(handle_client(stream, requests.clone(), self.options.timeouts, next_client));
                    }
                    Err(err) => tracing::warn!("Failed to accept connection: {err}"),
                },
                Some((key, reply)) = session_requests.recv() => {
                    let _ = reply.send(session(&mut sessions, key, &self.options));
                }
                Some(_) = clients.join_next() => last_activity = Instant::now(),
                () = sleep_until(last_activity + idle.unwrap_or_default()), if idle.is_some() => {
                    tracing::info!("Idle timeout reached, shutting down");
                    break;
                }
                _ = liveness.tick() => {
                    if !sessions.is_empty() && sessions.keys().all(|key| !key.root().exists()) {
                        tracing::info!("All workspace roots deleted, shutting down");
                        break;
                    }
                }
            }
        }
        join_all(sessions.values().map(WorkspaceSession::shutdown)).await;
        let _ = remove_file(&self.socket_path);
        let _ = remove_file(self.socket_path.with_extension("lock"));
        Ok(())
    }
}

fn session(
    sessions: &mut HashMap<WorkspaceKey, WorkspaceSession>,
    key: WorkspaceKey,
    options: &SpawnOptions,
) -> DaemonResult<WorkspaceSession> {
    if let Some(session) = sessions.get(&key).filter(|session| session.is_alive()) {
        return Ok(session.clone());
    }
    let session = WorkspaceSession::spawn(key.root(), key.server(), options)?;
    sessions.insert(key, session.clone());
    Ok(session)
}

#[tracing::instrument(skip(stream, sessions, timeouts))]
async fn handle_client(stream: UnixStream, sessions: mpsc::Sender<SessionRequest>, timeouts: Timeouts, client: u64) {
    let (reader, writer) = tokio::io::split(stream);
    let mut reader = FramedRead::new(reader, JsonFrames::<DaemonRequest>::default());
    let (responses, mut pending) = mpsc::channel::<DaemonResponse>(100);
    let writer = tokio::spawn(async move {
        let mut writer = FramedWrite::new(writer, JsonFrames::default());
        while let Some(response) = pending.recv().await {
            if writer.send(response).await.is_err() {
                break;
            }
        }
    });
    if let Some(Ok(DaemonRequest::Initialize { root, server })) = reader.next().await {
        let opened = open(sessions, &root, &server, timeouts).await;
        let initialized = opened.as_ref().map(|_| ()).map_err(Clone::clone);
        let _ = responses.send(DaemonResponse::Initialized(initialized)).await;
        if let Ok(workspace) = opened {
            let mut running = JoinSet::new();
            let mut cancels = HashMap::new();
            loop {
                select! {
                    request = reader.next() => match request {
                        Some(Ok(DaemonRequest::Call { id, call })) => {
                            let ordered = matches!(call, DaemonCall::QueueRefresh { .. });
                            let (workspace, responses) = (workspace.clone(), responses.clone());
                            let reply = async move {
                                let result = workspace.call(call).await;
                                let _ = responses.send(DaemonResponse::Reply { id, result }).await;
                                id
                            };
                            if ordered {
                                reply.await;
                            } else {
                                cancels.insert(id, running.spawn(reply));
                            }
                        }
                        Some(Ok(DaemonRequest::Cancel { id })) => {
                            if let Some(task) = cancels.remove(&id) {
                                task.abort();
                            }
                        }
                        _ => break,
                    },
                    Some(finished) = running.join_next() => {
                        if let Ok(id) = finished {
                            cancels.remove(&id);
                        }
                    }
                }
            }
            running.shutdown().await;
        }
    }
    drop(responses);
    let _ = writer.await;
}

async fn open(
    sessions: mpsc::Sender<SessionRequest>,
    root: &Path,
    server: &str,
    timeouts: Timeouts,
) -> Result<Workspace, String> {
    let key = WorkspaceKey::trusted(root, server).ok_or_else(|| format!("No language server named {server}"))?;
    let spec = key.server();
    Workspace::open(key, sessions, timeouts).await.map_err(|error| spec.unavailable(&error))
}

pub use entry::run_if_requested;

mod entry {
    use super::{DaemonError, LspDaemon, SpawnOptions, Timeouts};
    use crate::log_file::LogFile;
    use clap::Parser;
    use std::path::PathBuf;
    use std::process::ExitCode;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::select;
    use tokio::signal::unix::{SignalKind, signal};
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::fmt::writer::BoxMakeWriter;

    const QUIET_DEPENDENCIES: &str = "globset=warn,ignore=warn,notify=warn";
    const LOG_FILE_LIMIT: u64 = 10 * 1024 * 1024;

    #[derive(Debug, thiserror::Error)]
    enum RunError {
        #[error("Failed to create tokio runtime: {0}")]
        RuntimeCreate(std::io::Error),
        #[error("Failed to open log file: {0}")]
        LogFileOpen(std::io::Error),
        #[error("{0}")]
        Daemon(#[from] DaemonError),
    }

    #[derive(Debug, Parser)]
    #[command(name = "lspd")]
    struct Args {
        #[arg(long)]
        socket: PathBuf,

        #[arg(long, default_value = "300")]
        idle_timeout: u64,

        #[arg(long, env = "CLANKERDIFF_LSPD_REQUEST_TIMEOUT", default_value = "120", value_parser = clap::value_parser!(u64).range(1..))]
        request_timeout: u64,

        #[arg(long, default_value = "info")]
        log_level: String,

        #[arg(long)]
        log_file: Option<PathBuf>,
    }

    #[must_use]
    pub fn run_if_requested() -> Option<ExitCode> {
        if std::env::args_os().nth(1).is_none_or(|command| command != "lspd") {
            return None;
        }
        Some(match run(Args::parse_from(std::env::args_os().skip(1))) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err}");
                ExitCode::FAILURE
            }
        })
    }

    fn run(args: Args) -> Result<(), RunError> {
        let _ = nix::unistd::setsid();
        let idle_timeout = (args.idle_timeout > 0).then(|| Duration::from_secs(args.idle_timeout));
        let timeouts = Timeouts { request: Duration::from_secs(args.request_timeout), ..Timeouts::default() };
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(format!("{},{QUIET_DEPENDENCIES}", args.log_level)));
        let writer = match &args.log_file {
            Some(path) => {
                let file = LogFile::open(path, LOG_FILE_LIMIT).map_err(RunError::LogFileOpen)?;
                BoxMakeWriter::new(Arc::new(file))
            }
            None => BoxMakeWriter::new(std::io::stderr),
        };
        let runtime = tokio::runtime::Runtime::new().map_err(RunError::RuntimeCreate)?;
        runtime.block_on(async {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_target(true)
                .with_ansi(args.log_file.is_none())
                .with_writer(writer)
                .init();
            tracing::info!("Starting LSP daemon on socket: {:?}", args.socket);
            let daemon =
                LspDaemon::bind(args.socket, idle_timeout, SpawnOptions { timeouts, ..SpawnOptions::default() })?;
            let mut terminate = signal(SignalKind::terminate()).map_err(DaemonError::Io)?;
            let shutdown = async move {
                select! {
                    _ = terminate.recv() => tracing::info!("Received SIGTERM"),
                    _ = tokio::signal::ctrl_c() => tracing::info!("Received SIGINT"),
                }
            };
            Ok(daemon.run_until(shutdown).await?)
        })
    }
}
