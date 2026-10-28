#![doc = include_str!("docs/testing.md")]

use crate::client::{ClientError, LspClient};
use crate::daemon::{DaemonError, LspDaemon, SpawnOptions};
use crate::language_catalog::{LanguageId, ServerCommand};
use crate::workspace_key::WorkspaceKey;
use std::fmt::Debug;
use std::fs;
use std::future::Future;
use std::iter::once;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::oneshot;

pub use crate::hooks::{TestGate, TestPoint};

pub struct TestWorkspace {
    root: TempDir,
}

pub struct TestDaemon {
    workspace: WorkspaceKey,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<Result<(), DaemonError>>>,
    clock: Option<tokio::runtime::Handle>,
}

pub struct TestDaemonBuilder {
    workspace: WorkspaceKey,
    options: SpawnOptions,
    paused_time: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum TestDaemonError {
    #[error("Daemon failed: {0}")]
    Daemon(#[from] DaemonError),
    #[error("Failed to create daemon runtime: {0}")]
    Runtime(std::io::Error),
    #[error("Daemon thread panicked")]
    ThreadPanicked,
}

impl TestDaemon {
    pub fn spawn(workspace_root: &Path, language: LanguageId) -> Result<Self, TestDaemonError> {
        Self::builder(workspace_root, language).spawn()
    }

    #[must_use]
    pub fn builder(workspace_root: &Path, language: LanguageId) -> TestDaemonBuilder {
        TestDaemonBuilder {
            workspace: WorkspaceKey::resolve(workspace_root, language),
            options: SpawnOptions::default(),
            paused_time: false,
        }
    }

    pub async fn client(&self) -> Result<LspClient, ClientError> {
        let client = LspClient::new(self.workspace.clone());
        client.connect().await?;
        Ok(client)
    }

    pub async fn advance_time(&self, duration: Duration) {
        self.clock
            .as_ref()
            .expect("enable paused_time on the daemon builder")
            .spawn(async move { tokio::time::advance(duration).await })
            .await
            .expect("daemon clock task should complete");
    }

    pub fn shutdown(mut self) -> Result<(), TestDaemonError> {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        let task = self.task.take().expect("test daemon task should be present");
        task.join().map_err(|_| TestDaemonError::ThreadPanicked)??;
        Ok(())
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

impl TestDaemonBuilder {
    #[must_use]
    pub fn gate(mut self, point: TestPoint, gate: &TestGate) -> Self {
        self.options.hooks.0.push((point, gate.clone()));
        self
    }

    #[must_use]
    pub const fn paused_time(mut self) -> Self {
        self.paused_time = true;
        self
    }

    #[must_use]
    pub const fn request_timeout(mut self, timeout: Duration) -> Self {
        self.options.timeouts.request = timeout;
        self
    }

    #[must_use]
    pub const fn diagnostics_settle(mut self, settle: Duration) -> Self {
        self.options.timeouts.diagnostics_settle = settle;
        self
    }

    #[must_use]
    pub fn fake_server(mut self, extra_args: &[&str]) -> Self {
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/common/fake_lsp_server.py");
        let args = once(script.to_string_lossy().into_owned()).chain(extra_args.iter().map(ToString::to_string));
        self.options.command = Some(ServerCommand { command: "python3".to_owned(), args: args.collect() });
        self.diagnostics_settle(Duration::ZERO)
    }

    #[must_use]
    pub fn define(mut self, name: &str, path: &Path, line: usize) -> Self {
        let command = self.options.command.as_mut().expect("call fake_server before define");
        command.args.extend(["--define".to_owned(), format!("{name}={}:{line}", path.display())]);
        self
    }

    pub fn spawn(self) -> Result<TestDaemon, TestDaemonError> {
        let daemon = LspDaemon::bind(self.workspace.socket_path(), None, self.options)?;
        let runtime = if self.paused_time {
            tokio::runtime::Builder::new_current_thread().enable_all().start_paused(true).build()
        } else {
            tokio::runtime::Builder::new_multi_thread().enable_all().build()
        }
        .map_err(TestDaemonError::Runtime)?;
        let clock = self.paused_time.then(|| runtime.handle().clone());
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let task = std::thread::spawn(move || {
            let (release_tx, release_rx) = mpsc::channel::<()>();
            let hold_clock = self.paused_time.then(|| runtime.spawn_blocking(move || release_rx.recv()));
            let result = runtime.block_on(daemon.run_until(async {
                let _ = shutdown_rx.await;
            }));
            drop(release_tx);
            if let Some(hold_clock) = hold_clock {
                runtime.block_on(hold_clock).expect("clock guard should finish").ok();
            }
            result
        });
        Ok(TestDaemon { workspace: self.workspace, shutdown_tx: Some(shutdown_tx), task: Some(task), clock })
    }
}

impl TestGate {
    pub async fn reached(&self) {
        let _ = self.state.subscribe().wait_for(|(reached, _)| *reached).await;
    }

    pub async fn reached_before<T: Future<Output: Debug>>(&self, work: Pin<&mut T>) {
        tokio::select! {
            () = self.reached() => {}
            result = work => panic!("completed before the gate was reached: {result:?}"),
        }
    }

    pub fn release(&self) {
        self.state.send_modify(|(_, released)| *released = true);
    }
}

impl TestWorkspace {
    #[must_use]
    pub fn new() -> Self {
        Self { root: TempDir::new().expect("failed to create temp workspace") }
    }

    #[must_use]
    pub fn file(self, path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Self {
        self.write(path, contents);
        self
    }

    pub fn write(&self, path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.path(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("failed to create parent directory");
        }
        fs::write(&path, contents).expect("failed to write file");
        path
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        self.root.path()
    }

    #[must_use]
    pub fn path(&self, path: impl AsRef<Path>) -> PathBuf {
        self.root().join(path)
    }
}

impl Default for TestWorkspace {
    fn default() -> Self {
        Self::new()
    }
}
