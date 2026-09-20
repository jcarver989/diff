#[cfg(feature = "websocket")]
use crate::websocket::{self, ServerListener};
use crate::{
    connection,
    transport::{Encoded, ServerMessageTransport, Transport},
};
use clankerdiff_core::{DiffScope, ReviewSubmission};
use clankerdiff_git::{GitError, GitRepository};
use clankerdiff_protocol::{
    client::{LocalClientMessageTransport, LocalClientTransport},
    server::{local_message_transport_pair, local_transport_pair},
    shared::{RemoteError, RemoteErrorCode},
};
use clankerdiff_watch::{RepositoryHandle, RepositoryWatcher, WatchError, WatchOptions};
#[cfg(feature = "websocket")]
use std::future::Future;
#[cfg(feature = "websocket")]
use std::net::SocketAddr;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use thiserror::Error;
use tokio::{sync::mpsc, time::timeout};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewCompletion {
    Submitted {
        scope: DiffScope,
        submission: ReviewSubmission,
    },
    Cancelled {
        scope: DiffScope,
    },
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ServerOptions {
    pub watch: WatchOptions,
}

#[derive(Debug, Error)]
pub enum ServerError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Watch(#[from] WatchError),
    #[error("server stopped")]
    Stopped,
}

pub(crate) struct Context {
    pub repository: GitRepository,
    pub root: Arc<str>,
    pub watcher: RepositoryHandle,
    completion: CompletionSink,
}

struct CompletionSink {
    accepted: AtomicBool,
    sender: mpsc::Sender<ReviewCompletion>,
}

impl CompletionSink {
    fn reserve(&self) -> Result<(), RemoteError> {
        self.accepted
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| RemoteError::new(RemoteErrorCode::Busy, "review already completed"))
    }

    async fn publish(&self, completion: ReviewCompletion) -> Result<(), RemoteError> {
        self.sender.send(completion).await.map_err(|_| stopped())
    }
}

impl Context {
    pub fn reserve_completion(&self) -> Result<(), RemoteError> {
        self.completion.reserve()
    }

    pub async fn publish_completion(
        &self,
        completion: ReviewCompletion,
    ) -> Result<(), RemoteError> {
        self.completion.publish(completion).await
    }
}

#[derive(Clone)]
pub(crate) struct Handle {
    context: Arc<Context>,
    tasks: TaskTracker,
    stop: CancellationToken,
}

impl Handle {
    pub fn accept<T: Transport>(&self, transport: T, stop: CancellationToken) {
        let context = Arc::clone(&self.context);
        self.tasks
            .spawn(connection::accept(context, transport, stop));
    }

    #[cfg(feature = "websocket")]
    #[must_use]
    pub fn child_token(&self) -> CancellationToken {
        self.stop.child_token()
    }

    #[cfg(feature = "websocket")]
    pub fn spawn<T>(&self, future: T) -> tokio::task::JoinHandle<T::Output>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        self.tasks.spawn(future)
    }
}

pub struct DiffServer {
    handle: Handle,
    watcher: Option<RepositoryWatcher>,
    completions: mpsc::Receiver<ReviewCompletion>,
}

impl DiffServer {
    pub async fn open(path: impl AsRef<Path>, options: ServerOptions) -> Result<Self, ServerError> {
        let repository = GitRepository::discover(path).await?;
        let watcher =
            RepositoryWatcher::spawn(repository.clone(), DiffScope::Both, options.watch).await?;
        let (completion_tx, completions) = mpsc::channel(1);
        let handle = Handle {
            context: Arc::new(Context {
                root: Arc::from(repository.root().to_string_lossy().as_ref()),
                repository,
                watcher: watcher.handle(),
                completion: CompletionSink {
                    accepted: AtomicBool::new(false),
                    sender: completion_tx,
                },
            }),
            tasks: TaskTracker::new(),
            stop: CancellationToken::new(),
        };
        Ok(Self {
            handle,
            watcher: Some(watcher),
            completions,
        })
    }

    #[must_use]
    pub fn repository_root(&self) -> &str {
        &self.handle.context.root
    }

    pub async fn next_review(&mut self) -> Result<ReviewCompletion, ServerError> {
        self.completions.recv().await.ok_or(ServerError::Stopped)
    }

    pub fn connect(&self) -> Result<LocalClientTransport, ServerError> {
        let (client, server) = local_transport_pair(4);
        self.accept_transport(server)?;
        Ok(client)
    }

    pub fn connect_messages(&self) -> Result<LocalClientMessageTransport, ServerError> {
        let (client, server) = local_message_transport_pair(100);
        self.accept(server)?;
        Ok(client)
    }

    pub fn accept(&self, transport: impl ServerMessageTransport) -> Result<(), ServerError> {
        self.accept_transport(Encoded::new(transport))
    }

    fn accept_transport<T: Transport>(&self, transport: T) -> Result<(), ServerError> {
        if self.handle.stop.is_cancelled() {
            return Err(ServerError::Stopped);
        }
        self.handle.accept(transport, self.handle.stop.clone());
        Ok(())
    }

    #[cfg(feature = "websocket")]
    pub async fn listen(&self, address: SocketAddr) -> Result<ServerListener, ServerError> {
        websocket::listen(self.handle.clone(), address).await
    }

    pub async fn shutdown(mut self) -> Result<(), ServerError> {
        self.handle.stop.cancel();
        self.handle.tasks.close();
        if timeout(SHUTDOWN_GRACE, self.handle.tasks.wait())
            .await
            .is_err()
        {
            return Err(ServerError::Stopped);
        }
        if let Some(watcher) = self.watcher.take() {
            watcher.shutdown().await?;
        }
        Ok(())
    }
}

impl Drop for DiffServer {
    fn drop(&mut self) {
        self.handle.stop.cancel();
        self.handle.tasks.close();
    }
}

pub(crate) fn stopped() -> RemoteError {
    RemoteError::new(RemoteErrorCode::Cancelled, "server stopped")
}

pub(crate) fn protocol(message: impl ToString) -> RemoteError {
    let text = message.to_string();
    drop(message);
    RemoteError::new(RemoteErrorCode::Protocol, text)
}
