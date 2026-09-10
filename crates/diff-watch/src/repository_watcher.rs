use crate::{
    error::WatchError,
    file_watcher::{FileWatchError, FileWatcher, NotifyFileWatcher},
    filter::should_refresh,
};
use clankerdiff_core::DiffScope;
use clankerdiff_git::{GitError, GitRepository, RepositorySnapshot};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchOptions {
    pub debounce: Duration,
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(150),
        }
    }
}

#[derive(Debug)]
pub enum RepositoryRequest {
    SetScope {
        scope: DiffScope,
        result_tx: oneshot::Sender<Result<(), Arc<GitError>>>,
    },
}

/// The retained repository state: the last successful snapshot and the health of
/// the most recent load. A failed load keeps the previous snapshot in place.
#[derive(Debug, Clone)]
pub struct RepositoryState {
    pub snapshot: Arc<RepositorySnapshot>,
    pub error: Option<Arc<GitError>>,
}

impl RepositoryState {
    /// The most recent load failure as a display message, if any.
    #[must_use]
    pub fn error_message(&self) -> Option<String> {
        self.error.as_ref().map(ToString::to_string)
    }

    /// Folds one load result in; returns whether anything observable changed.
    fn apply(&mut self, result: Result<RepositorySnapshot, Arc<GitError>>) -> bool {
        match result {
            Ok(snapshot) => {
                let recovered = self.error.take().is_some();
                if *self.snapshot == snapshot {
                    return recovered;
                }
                self.snapshot = Arc::new(snapshot);
                true
            }
            Err(error) => {
                if self.error_message() == Some(error.to_string()) {
                    return false;
                }
                self.error = Some(error);
                true
            }
        }
    }
}

#[derive(Debug)]
pub struct RepositoryWatcher {
    pub request_tx: mpsc::Sender<RepositoryRequest>,
    pub state_rx: watch::Receiver<RepositoryState>,
    task: JoinHandle<()>,
}

impl RepositoryWatcher {
    pub async fn spawn(
        repository: GitRepository,
        scope: DiffScope,
        options: WatchOptions,
    ) -> Result<Self, WatchError> {
        let watcher = Self::create_file_watcher(&repository, options.debounce).await?;
        let snapshot = repository.snapshot_with_sources(scope).await?;
        let (request_tx, request_rx) = mpsc::channel(64);
        let (state_tx, state_rx) = watch::channel(RepositoryState {
            snapshot: Arc::new(snapshot),
            error: None,
        });
        let actor = RepositoryActor {
            repository,
            watcher,
            scope,
            request_rx,
            state_tx,
        };
        Ok(Self {
            request_tx,
            state_rx,
            task: tokio::spawn(actor.run()),
        })
    }

    async fn create_file_watcher(
        repository: &GitRepository,
        debounce: Duration,
    ) -> Result<NotifyFileWatcher, WatchError> {
        let root = repository.root().to_path_buf();
        let directories = repository.metadata_directories().await?;
        let mut roots = vec![root.clone()];
        roots.extend(directories.iter().cloned());
        let filter_repository = repository.clone();
        let watcher = NotifyFileWatcher::new(roots, debounce, move |paths| {
            let repository = filter_repository.clone();
            let directories = directories.clone();
            async move { should_refresh(&repository, &directories, paths).await }
        })
        .map_err(|error| match error {
            FileWatchError::Create(source) => WatchError::Watch { path: root, source },
            FileWatchError::Watch { path, source } => WatchError::Watch { path, source },
        })?;

        Ok(watcher)
    }
}

impl Drop for RepositoryWatcher {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct RepositoryActor {
    repository: GitRepository,
    watcher: NotifyFileWatcher,
    scope: DiffScope,
    request_rx: mpsc::Receiver<RepositoryRequest>,
    state_tx: watch::Sender<RepositoryState>,
}

impl RepositoryActor {
    async fn run(mut self) {
        loop {
            let result_tx = tokio::select! {
                biased;
                request = self.request_rx.recv() => match request {
                    Some(RepositoryRequest::SetScope { scope, result_tx }) => {
                        self.scope = scope;
                        Some(result_tx)
                    }
                    None => return,
                },
                Some(()) = self.watcher.recv() => None,
            };

            let result = self
                .repository
                .snapshot_with_sources(self.scope)
                .await
                .map_err(Arc::new);
            let outcome = result.as_ref().map(|_| ()).map_err(Arc::clone);
            self.state_tx.send_if_modified(|state| state.apply(result));

            if let Some(tx) = result_tx {
                let _ = tx.send(outcome);
            }
        }
    }
}
