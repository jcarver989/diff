use crate::{
    error::WatchError,
    file_watcher::{FileWatchError, FileWatcher, NotifyFileWatcher},
    filter::should_refresh,
};
use diff_core::DiffScope;
use diff_git::{GitError, GitRepository, RepositorySnapshot};
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

#[derive(Debug)]
pub struct RepositoryWatcher {
    pub request_tx: mpsc::Sender<RepositoryRequest>,
    pub snapshot_rx: watch::Receiver<Result<Arc<RepositorySnapshot>, Arc<GitError>>>,
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
        let (state_tx, state_rx) = watch::channel(Ok(Arc::new(snapshot)));
        let actor = RepositoryActor {
            repository,
            watcher,
            scope,
            request_rx,
            state_tx,
        };
        Ok(Self {
            request_tx,
            snapshot_rx: state_rx,
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
    state_tx: watch::Sender<Result<Arc<RepositorySnapshot>, Arc<GitError>>>,
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
                .map(Arc::new)
                .map_err(Arc::new);

            self.state_tx.send_if_modified(|state| {
                let unchanged = match (&*state, &result) {
                    (Ok(previous), Ok(snapshot)) => previous == snapshot,
                    (Err(previous), Err(error)) => previous.to_string() == error.to_string(),
                    _ => false,
                };

                if unchanged {
                    return false;
                }

                state.clone_from(&result);
                true
            });

            if let Some(tx) = result_tx {
                let _ = tx.send(result.map(|_| ()));
            }
        }
    }
}
