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
    Refresh {
        result_tx: oneshot::Sender<Result<(), Arc<GitError>>>,
    },
    RefreshScope {
        scope: DiffScope,
        result_tx: oneshot::Sender<Result<(), Arc<GitError>>>,
    },
    Shutdown,
    Subscribe {
        scope: DiffScope,
        result_tx: oneshot::Sender<Result<watch::Receiver<RepositoryState>, Arc<GitError>>>,
    },
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

#[derive(Debug, thiserror::Error)]
pub enum RepositoryHandleError {
    #[error(transparent)]
    Git(#[from] Arc<GitError>),
    #[error("the repository watcher has stopped")]
    Stopped,
}

#[derive(Debug, Clone)]
pub struct RepositoryHandle {
    request_tx: mpsc::Sender<RepositoryRequest>,
}

impl RepositoryHandle {
    pub async fn subscribe(
        &self,
        scope: DiffScope,
    ) -> Result<watch::Receiver<RepositoryState>, RepositoryHandleError> {
        let (result_tx, result_rx) = oneshot::channel();
        self.request_tx
            .send(RepositoryRequest::Subscribe { scope, result_tx })
            .await
            .map_err(|_| RepositoryHandleError::Stopped)?;
        result_rx
            .await
            .map_err(|_| RepositoryHandleError::Stopped)?
            .map_err(RepositoryHandleError::Git)
    }

    pub async fn refresh(&self, scope: DiffScope) -> Result<(), RepositoryHandleError> {
        let (result_tx, result_rx) = oneshot::channel();
        self.request_tx
            .send(RepositoryRequest::RefreshScope { scope, result_tx })
            .await
            .map_err(|_| RepositoryHandleError::Stopped)?;
        result_rx
            .await
            .map_err(|_| RepositoryHandleError::Stopped)?
            .map_err(RepositoryHandleError::Git)
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
            subscriptions: [None, None, None],
        };
        Ok(Self {
            request_tx,
            state_rx,
            task: tokio::spawn(actor.run()),
        })
    }

    #[must_use]
    pub fn handle(&self) -> RepositoryHandle {
        RepositoryHandle {
            request_tx: self.request_tx.clone(),
        }
    }

    pub async fn shutdown(mut self) -> Result<(), WatchError> {
        self.request_tx
            .send(RepositoryRequest::Shutdown)
            .await
            .map_err(|_| WatchError::Stopped)?;
        (&mut self.task).await.map_err(|_| WatchError::Stopped)
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
    subscriptions: [Option<watch::Sender<RepositoryState>>; 3],
}

impl RepositoryActor {
    async fn run(mut self) {
        loop {
            tokio::select! {
                biased;
                request = self.request_rx.recv() => match request {
                    Some(RepositoryRequest::SetScope { scope, result_tx }) => {
                        self.scope = scope;
                        let _ = result_tx.send(self.refresh_scope(scope, true).await);
                    }
                    Some(RepositoryRequest::Refresh { result_tx }) => {
                        let _ = result_tx.send(self.refresh_scope(self.scope, true).await);
                    }
                    Some(RepositoryRequest::RefreshScope { scope, result_tx }) => {
                        let _ = result_tx.send(self.refresh_scope(scope, true).await);
                    }
                    Some(RepositoryRequest::Subscribe { scope, result_tx }) => {
                        let _ = result_tx.send(self.subscribe(scope).await);
                    }
                    Some(RepositoryRequest::Shutdown) | None => return,
                },
                event = self.watcher.recv() => match event {
                    Some(()) => self.refresh_active().await,
                    None => return,
                },
            }
        }
    }

    async fn subscribe(
        &mut self,
        scope: DiffScope,
    ) -> Result<watch::Receiver<RepositoryState>, Arc<GitError>> {
        if scope == self.scope {
            return Ok(self.state_tx.subscribe());
        }
        let index = scope_index(scope);
        if let Some(state) = &self.subscriptions[index] {
            return Ok(state.subscribe());
        }
        let snapshot = self
            .repository
            .snapshot_with_sources(scope)
            .await
            .map_err(Arc::new)?;
        let (state, receiver) = watch::channel(RepositoryState {
            snapshot: Arc::new(snapshot),
            error: None,
        });
        self.subscriptions[index] = Some(state);
        Ok(receiver)
    }

    async fn refresh_active(&mut self) {
        let scopes = [DiffScope::Unstaged, DiffScope::Staged, DiffScope::Both];
        for scope in scopes {
            let subscribed = self.subscriptions[scope_index(scope)]
                .as_ref()
                .is_some_and(|state| state.receiver_count() > 0);
            if scope == self.scope || subscribed {
                let _ = self.refresh_scope(scope, false).await;
            } else {
                self.subscriptions[scope_index(scope)] = None;
            }
        }
    }

    async fn refresh_scope(&mut self, scope: DiffScope, retry: bool) -> Result<(), Arc<GitError>> {
        let result = if retry {
            self.repository.snapshot_with_sources(scope).await
        } else {
            match self.repository.try_snapshot_with_sources(scope).await {
                Err(GitError::UnstableSnapshot) => {
                    self.repository
                        .snapshot(scope)
                        .await
                        .map(|document| RepositorySnapshot {
                            scope,
                            document: Arc::new(document),
                        })
                }
                result => result,
            }
        }
        .map_err(Arc::new);
        let outcome = result.as_ref().map(|_| ()).map_err(Arc::clone);
        if scope == self.scope {
            self.state_tx
                .send_if_modified(|state| state.apply(clone_result(&result)));
        }
        if let Some(state) = &self.subscriptions[scope_index(scope)] {
            state.send_if_modified(|current| current.apply(clone_result(&result)));
        }
        outcome
    }
}

fn clone_result(
    result: &Result<RepositorySnapshot, Arc<GitError>>,
) -> Result<RepositorySnapshot, Arc<GitError>> {
    result.clone()
}

const fn scope_index(scope: DiffScope) -> usize {
    match scope {
        DiffScope::Unstaged => 0,
        DiffScope::Staged => 1,
        DiffScope::Both => 2,
    }
}
