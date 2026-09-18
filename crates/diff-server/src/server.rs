use crate::{
    connection::{self, ServerTransport},
    websocket::{self, ServerListener},
};
use clankerdiff_core::DiffScope;
use clankerdiff_git::{GitError, GitRepository};
use clankerdiff_protocol::{
    client::LocalClientTransport,
    server::local_transport_pair,
    shared::{RemoteError, RemoteErrorCode},
};
use clankerdiff_watch::{RepositoryHandle, RepositoryWatcher, WatchError, WatchOptions};
use std::{future::Future, net::SocketAddr, path::Path, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::time::timeout;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

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
}

#[derive(Clone)]
pub(crate) struct Handle {
    context: Arc<Context>,
    tasks: TaskTracker,
    stop: CancellationToken,
}

impl Handle {
    pub fn accept(&self, transport: ServerTransport, stop: CancellationToken) {
        let context = Arc::clone(&self.context);
        self.tasks
            .spawn(connection::accept(context, transport, stop));
    }

    #[must_use]
    pub fn child_token(&self) -> CancellationToken {
        self.stop.child_token()
    }

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
}

impl DiffServer {
    pub async fn open(path: impl AsRef<Path>, options: ServerOptions) -> Result<Self, ServerError> {
        let repository = GitRepository::discover(path).await?;
        let watcher =
            RepositoryWatcher::spawn(repository.clone(), DiffScope::Both, options.watch).await?;
        let handle = Handle {
            context: Arc::new(Context {
                root: Arc::from(repository.root().to_string_lossy().as_ref()),
                repository,
                watcher: watcher.handle(),
            }),
            tasks: TaskTracker::new(),
            stop: CancellationToken::new(),
        };
        Ok(Self {
            handle,
            watcher: Some(watcher),
        })
    }

    pub fn connect(&self) -> Result<LocalClientTransport, ServerError> {
        let (client, server) = local_transport_pair(4);
        if self.handle.stop.is_cancelled() {
            return Err(ServerError::Stopped);
        }
        self.handle
            .accept(ServerTransport::Local(server), self.handle.stop.clone());
        Ok(client)
    }

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
