use crate::{
    error::WatchError,
    file_watcher::{FileWatcher, NotifyFileWatcher, worktree_walk},
    filter::should_refresh,
};
use clankerdiff_core::DiffScope;
use clankerdiff_git::{GitError, GitRepository, RepositorySnapshot};
use ignore::WalkBuilder;
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::{JoinHandle, spawn_blocking},
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
        let directories = repository.metadata_directories().await?;
        let mut worktree = worktree_walk(repository.root());
        worktree.filter_entry(|entry| entry.file_name() != ".git");
        let mut walks = vec![worktree];
        if let Some((first, rest)) = directories.split_first() {
            let mut metadata = WalkBuilder::new(first);
            for directory in rest {
                metadata.add(directory);
            }
            metadata.standard_filters(false).filter_entry(|entry| {
                entry.depth() != 1 || matches!(entry.file_name().to_str(), Some("refs" | "info"))
            });
            walks.push(metadata);
        }
        let repository = repository.clone();
        let watcher = spawn_blocking(move || {
            NotifyFileWatcher::with_walks(walks, debounce, move |paths| {
                let repository = repository.clone();
                let directories = directories.clone();
                async move { should_refresh(&repository, &directories, paths).await }
            })
        })
        .await
        .map_err(|_| WatchError::Stopped)??;
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
                event = self.watcher.recv() => match event {
                    Some(()) => None,
                    None => return,
                },
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
