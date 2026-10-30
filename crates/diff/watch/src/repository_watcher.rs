use crate::{
    error::WatchError,
    file_watcher::{FileWatcher, NotifyFileWatcher, worktree_walk},
    filter::should_refresh,
};
use clankerdiff_core::{DiffDocument, RepoPath, ReviewTarget};
use clankerdiff_git::{GitError, GitRepository};
use ignore::WalkBuilder;
use std::{collections::HashMap, sync::Arc, time::Duration};
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
        Self { debounce: Duration::from_millis(150) }
    }
}

#[derive(Debug)]
pub enum RepositoryRequest {
    Refresh {
        result_tx: oneshot::Sender<Result<(), Arc<GitError>>>,
    },
    RefreshTarget {
        target: ReviewTarget,
        result_tx: oneshot::Sender<Result<(), Arc<GitError>>>,
    },
    Shutdown,
    Subscribe {
        target: ReviewTarget,
        result_tx: oneshot::Sender<Result<watch::Receiver<RepositoryState>, Arc<GitError>>>,
    },
    SetTarget {
        target: ReviewTarget,
        result_tx: oneshot::Sender<Result<(), Arc<GitError>>>,
    },
}

/// The retained repository state: the last successful snapshot and the health of
/// the most recent load. A failed load keeps the previous snapshot in place.
#[derive(Debug, Clone)]
pub struct RepositoryState {
    pub snapshot: Arc<DiffDocument>,
    pub files: Arc<[RepoPath]>,
    pub error: Option<Arc<GitError>>,
}

impl RepositoryState {
    /// The most recent load failure as a display message, if any.
    #[must_use]
    pub fn error_message(&self) -> Option<String> {
        self.error.as_ref().map(ToString::to_string)
    }

    fn new((snapshot, files): Loaded) -> Self {
        Self { snapshot: Arc::new(snapshot), files: files.into(), error: None }
    }

    /// Folds one load result in; returns whether anything observable changed.
    fn apply(&mut self, result: Result<Loaded, Arc<GitError>>) -> bool {
        match result {
            Ok((snapshot, files)) => {
                let mut changed = self.error.take().is_some();
                if *self.snapshot != snapshot {
                    self.snapshot = Arc::new(snapshot);
                    changed = true;
                }
                if *self.files != *files {
                    self.files = files.into();
                    changed = true;
                }
                changed
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
        target: impl Into<ReviewTarget>,
    ) -> Result<watch::Receiver<RepositoryState>, RepositoryHandleError> {
        let (result_tx, result_rx) = oneshot::channel();
        self.request_tx
            .send(RepositoryRequest::Subscribe { target: target.into(), result_tx })
            .await
            .map_err(|_| RepositoryHandleError::Stopped)?;
        result_rx.await.map_err(|_| RepositoryHandleError::Stopped)?.map_err(RepositoryHandleError::Git)
    }

    pub async fn refresh(&self, target: impl Into<ReviewTarget>) -> Result<(), RepositoryHandleError> {
        let (result_tx, result_rx) = oneshot::channel();
        self.request_tx
            .send(RepositoryRequest::RefreshTarget { target: target.into(), result_tx })
            .await
            .map_err(|_| RepositoryHandleError::Stopped)?;
        result_rx.await.map_err(|_| RepositoryHandleError::Stopped)?.map_err(RepositoryHandleError::Git)
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
        target: impl Into<ReviewTarget>,
        options: WatchOptions,
    ) -> Result<Self, WatchError> {
        let target = target.into();
        let watcher = Self::create_file_watcher(&repository, options.debounce).await?;
        let loaded = load(&repository, &target, true).await?;
        let (request_tx, request_rx) = mpsc::channel(64);
        let (state_tx, state_rx) = watch::channel(RepositoryState::new(loaded));
        let actor =
            RepositoryActor { repository, watcher, target, request_rx, state_tx, subscriptions: HashMap::new() };
        Ok(Self { request_tx, state_rx, task: tokio::spawn(actor.run()) })
    }

    #[must_use]
    pub fn handle(&self) -> RepositoryHandle {
        RepositoryHandle { request_tx: self.request_tx.clone() }
    }

    pub async fn shutdown(mut self) -> Result<(), WatchError> {
        self.request_tx.send(RepositoryRequest::Shutdown).await.map_err(|_| WatchError::Stopped)?;
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
    target: ReviewTarget,
    request_rx: mpsc::Receiver<RepositoryRequest>,
    state_tx: watch::Sender<RepositoryState>,
    subscriptions: HashMap<ReviewTarget, watch::Sender<RepositoryState>>,
}

impl RepositoryActor {
    async fn run(mut self) {
        loop {
            tokio::select! {
                biased;
                request = self.request_rx.recv() => match request {
                    Some(RepositoryRequest::SetTarget { target, result_tx }) => {
                        self.target = target.clone();
                        let _ = result_tx.send(self.refresh_target(&target, true).await);
                    }
                    Some(RepositoryRequest::Refresh { result_tx }) => {
                        let target = self.target.clone();
                        let _ = result_tx.send(self.refresh_target(&target, true).await);
                    }
                    Some(RepositoryRequest::RefreshTarget { target, result_tx }) => {
                        let _ = result_tx.send(self.refresh_target(&target, true).await);
                    }
                    Some(RepositoryRequest::Subscribe { target, result_tx }) => {
                        let _ = result_tx.send(self.subscribe(target).await);
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

    async fn subscribe(&mut self, target: ReviewTarget) -> Result<watch::Receiver<RepositoryState>, Arc<GitError>> {
        if target == self.target {
            return Ok(self.state_tx.subscribe());
        }
        if let Some(state) = self.subscriptions.get(&target) {
            return Ok(state.subscribe());
        }
        let loaded = load(&self.repository, &target, true).await.map_err(Arc::new)?;
        let (state, receiver) = watch::channel(RepositoryState::new(loaded));
        self.subscriptions.insert(target, state);
        Ok(receiver)
    }

    async fn refresh_active(&mut self) {
        self.subscriptions.retain(|_, state| state.receiver_count() > 0);
        let targets = self
            .subscriptions
            .keys()
            .filter(|target| **target != self.target)
            .chain([&self.target])
            .filter(|target| target.scope().is_some())
            .cloned()
            .collect::<Vec<_>>();
        for target in targets {
            let _ = self.refresh_target(&target, false).await;
        }
    }

    async fn refresh_target(&mut self, target: &ReviewTarget, retry: bool) -> Result<(), Arc<GitError>> {
        let result = load(&self.repository, target, retry).await.map_err(Arc::new);
        let outcome = result.as_ref().map(|_| ()).map_err(Arc::clone);
        if *target == self.target {
            self.state_tx.send_if_modified(|state| state.apply(clone_result(&result)));
        }
        if let Some(state) = self.subscriptions.get(target) {
            state.send_if_modified(|current| current.apply(clone_result(&result)));
        }
        outcome
    }
}

type Loaded = (DiffDocument, Vec<RepoPath>);

async fn load(repository: &GitRepository, target: &ReviewTarget, retry: bool) -> Result<Loaded, GitError> {
    let snapshot = async {
        if retry {
            return repository.snapshot_with_sources(target.clone()).await;
        }
        match repository.try_snapshot_with_sources(target.clone()).await {
            Err(GitError::UnstableSnapshot) => repository.snapshot(target.clone()).await,
            result => result,
        }
    };
    let (snapshot, files) = tokio::join!(snapshot, repository.worktree_files());
    Ok((snapshot?, files?))
}

fn clone_result(result: &Result<Loaded, Arc<GitError>>) -> Result<Loaded, Arc<GitError>> {
    result.clone()
}
