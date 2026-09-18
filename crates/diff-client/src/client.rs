use crate::{
    ClientError, ClientOptions, ClientState, ConnectionState, DiffReviewEvent, DiffScope,
    DiffSnapshot, ReconnectPolicy, RemoteError, RepositoryAction, platform,
    protocol::{
        client::{ClientCommand, LocalClientTransport, capabilities},
        server::ServerEvent,
        shared::{Event, LIVE_PROTOCOL_VERSION, RemoteErrorCode},
    },
    transport::ClientTransport,
};
use async_channel::{Receiver, Sender, unbounded};
use futures_util::FutureExt;
use std::{sync::Arc, time::Duration};
use tokio::sync::{oneshot, watch};

const IDLE_TIMEOUT: Duration = Duration::from_secs(45);

type Reply = oneshot::Sender<Result<(), ClientError>>;

#[derive(Clone)]
pub struct DiffClient {
    commands: Sender<Command>,
    state: watch::Receiver<Arc<ClientState>>,
}

pub struct ClientSubscription {
    state: watch::Receiver<Arc<ClientState>>,
}

impl ClientSubscription {
    #[must_use]
    pub fn latest(&self) -> Arc<ClientState> {
        self.state.borrow().clone()
    }

    pub async fn changed(&mut self) -> Result<Arc<ClientState>, ClientError> {
        self.state
            .changed()
            .await
            .map_err(|_| ClientError::Disconnected)?;
        Ok(self.state.borrow_and_update().clone())
    }

    /// Waits until the published state satisfies `predicate`, or `timeout` elapses.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::Disconnected`] when the client stops publishing or
    /// the timeout elapses first.
    pub async fn wait_until(
        &mut self,
        timeout: Duration,
        mut predicate: impl FnMut(&ClientState) -> bool,
    ) -> Result<Arc<ClientState>, ClientError> {
        let deadline = platform::sleep(timeout).fuse();
        futures_util::pin_mut!(deadline);
        loop {
            let state = self.latest();
            if predicate(&state) {
                return Ok(state);
            }
            let changed = self.changed().fuse();
            futures_util::pin_mut!(changed);
            futures_util::select! {
                state = changed => { state?; }
                () = deadline => return Err(ClientError::Disconnected),
            }
        }
    }
}

impl DiffClient {
    #[cfg(feature = "websocket")]
    pub async fn connect(url: &str, options: ClientOptions) -> Result<Self, ClientError> {
        let transport = ClientTransport::try_connect(url).await?;
        Self::start(transport, options, Some(url.to_owned())).await
    }

    pub async fn from_transport(
        transport: LocalClientTransport,
        options: ClientOptions,
    ) -> Result<Self, ClientError> {
        Self::start(ClientTransport::Local(transport), options, None).await
    }

    async fn start(
        transport: ClientTransport,
        options: ClientOptions,
        url: Option<String>,
    ) -> Result<Self, ClientError> {
        let (commands, rx) = unbounded();
        let (state_tx, state) = watch::channel(Arc::new(ClientState::default()));
        let (ready_tx, ready_rx) = oneshot::channel();
        platform::spawn(async move {
            Worker {
                commands: rx,
                state_tx,
                options,
                state: WorkerState::default(),
                connection: ConnectionState::Connecting,
                closed: false,
            }
            .run(transport, url, ready_tx)
            .await;
        });
        ready_rx.await.map_err(|_| ClientError::Disconnected)??;
        Ok(Self { commands, state })
    }

    #[must_use]
    pub fn state(&self) -> Arc<ClientState> {
        self.state.borrow().clone()
    }

    #[must_use]
    pub fn subscribe(&self) -> ClientSubscription {
        ClientSubscription {
            state: self.state.clone(),
        }
    }

    pub async fn set_scope(&self, scope: DiffScope) -> Result<(), ClientError> {
        self.request(ClientCommand::SetScope(scope)).await
    }

    pub async fn apply(&self, action: RepositoryAction) -> Result<(), ClientError> {
        self.request(ClientCommand::Apply(action)).await
    }

    pub async fn refresh(&self) -> Result<(), ClientError> {
        self.request(ClientCommand::Refresh).await
    }

    pub async fn handle(&self, event: DiffReviewEvent) -> Result<(), ClientError> {
        match self.dispatch(event)? {
            Some(reply) => reply.await.map_err(|_| ClientError::Disconnected)?,
            None => Ok(()),
        }
    }

    pub fn dispatch(
        &self,
        event: DiffReviewEvent,
    ) -> Result<Option<oneshot::Receiver<Result<(), ClientError>>>, ClientError> {
        let request = match event {
            DiffReviewEvent::RepositoryAction(action) => ClientCommand::Apply(action),
            DiffReviewEvent::SetScope(scope) => ClientCommand::SetScope(scope),
            DiffReviewEvent::Refresh => ClientCommand::Refresh,
            _ => return Ok(None),
        };
        let (reply, receiver) = oneshot::channel();
        self.commands
            .try_send(Command::Request(request, reply))
            .map_err(|_| ClientError::Disconnected)?;
        Ok(Some(receiver))
    }

    pub async fn close(self) -> Result<(), ClientError> {
        self.command(Command::Close).await
    }

    async fn request(&self, request: ClientCommand) -> Result<(), ClientError> {
        self.command(|reply| Command::Request(request, reply)).await
    }

    async fn command(&self, build: impl FnOnce(Reply) -> Command) -> Result<(), ClientError> {
        let (reply, rx) = oneshot::channel();
        self.commands
            .send(build(reply))
            .await
            .map_err(|_| ClientError::Disconnected)?;
        rx.await.map_err(|_| ClientError::Disconnected)?
    }
}

enum Command {
    Request(ClientCommand, Reply),
    Close(Reply),
}

enum ConnectionInput {
    Command(Option<Command>),
    Event(Result<ServerEvent, ClientError>),
    Idle,
}

struct Pending {
    reply: Reply,
    action: bool,
}

struct Worker {
    commands: Receiver<Command>,
    state_tx: watch::Sender<Arc<ClientState>>,
    options: ClientOptions,
    state: WorkerState,
    connection: ConnectionState,
    closed: bool,
}

#[derive(Default)]
struct WorkerState {
    snapshot: Option<Arc<DiffSnapshot>>,
    error: Option<RemoteError>,
    pending: Option<Pending>,
    initialized: bool,
}

impl Worker {
    fn publish(&self) {
        let connected = matches!(self.connection, ConnectionState::Connected);
        self.state_tx.send_replace(Arc::new(ClientState {
            capabilities: capabilities(connected, self.state.snapshot.is_some()),
            snapshot: self.state.snapshot.clone(),
            connection: self.connection.clone(),
            error: self.state.error.clone(),
        }));
    }

    async fn run(mut self, mut transport: ClientTransport, url: Option<String>, ready: Reply) {
        let mut ready = Some(ready);
        loop {
            let result = self.connection(&mut transport, &mut ready).await;
            transport.close().await;
            self.abandon_request();
            let retry = !self.closed
                && ready.is_none()
                && url.is_some()
                && matches!(self.options.reconnect, ReconnectPolicy::Retry)
                && !is_terminal(&result);
            if let Some(ready) = ready.take() {
                let _ = ready.send(result.clone());
            }
            if !retry {
                self.connection =
                    ConnectionState::Failed(result.err().unwrap_or(ClientError::Disconnected));
                self.publish();
                return;
            }
            self.connection = ConnectionState::Connecting;
            self.publish();
            match self.reconnect(url.as_deref().unwrap_or_default()).await {
                Some(next) => transport = next,
                None => break,
            }
        }
        self.connection = ConnectionState::Failed(ClientError::Disconnected);
        self.publish();
    }

    async fn connection(
        &mut self,
        transport: &mut ClientTransport,
        ready: &mut Option<Reply>,
    ) -> Result<(), ClientError> {
        self.state.initialized = false;
        transport
            .send(ClientCommand::Initialize {
                protocol_version: LIVE_PROTOCOL_VERSION,
                scope: self.options.scope,
            })
            .await
            .map_err(|_| ClientError::Disconnected)?;
        let idle = platform::sleep(IDLE_TIMEOUT).fuse();
        futures_util::pin_mut!(idle);
        loop {
            let input = {
                let command = async {
                    if self.state.pending.is_none() {
                        self.commands.recv().await.ok()
                    } else {
                        futures_util::future::pending().await
                    }
                }
                .fuse();
                let event = transport.recv().fuse();
                futures_util::pin_mut!(command, event);
                futures_util::select! {
                    command = command => ConnectionInput::Command(command),
                    event = event => ConnectionInput::Event(event),
                    () = idle => ConnectionInput::Idle,
                }
            };
            match input {
                ConnectionInput::Command(None) => {
                    self.closed = true;
                    return Ok(());
                }
                ConnectionInput::Command(Some(Command::Close(reply))) => {
                    self.closed = true;
                    let _ = reply.send(Ok(()));
                    return Ok(());
                }
                ConnectionInput::Command(Some(Command::Request(request, reply))) => {
                    self.request(transport, request, reply).await?;
                }
                ConnectionInput::Event(event) => {
                    self.event(event?)?;
                    if self.state.initialized
                        && let Some(ready) = ready.take()
                    {
                        let _ = ready.send(Ok(()));
                    }
                }
                ConnectionInput::Idle => return Err(ClientError::Disconnected),
            }
            idle.set(platform::sleep(IDLE_TIMEOUT).fuse());
        }
    }

    fn event(&mut self, event: ServerEvent) -> Result<(), ClientError> {
        match event {
            Event::Initialize {
                protocol_version, ..
            } => {
                if protocol_version != LIVE_PROTOCOL_VERSION {
                    return Err(ClientError::Remote(RemoteError::new(
                        RemoteErrorCode::UnsupportedVersion,
                        "unsupported live protocol version",
                    )));
                }
                if std::mem::replace(&mut self.state.initialized, true) {
                    return Err(ClientError::Protocol("repeated initialization".to_owned()));
                }
                self.state.error = None;
                self.publish();
                Ok(())
            }
            Event::Error(error) => Err(ClientError::Remote(error)),
            _ if !self.state.initialized => {
                Err(ClientError::Protocol("expected initialization".to_owned()))
            }
            Event::Document(snapshot) => {
                self.state.snapshot = Some(snapshot);
                self.connection = ConnectionState::Connected;
                self.publish();
                Ok(())
            }
            Event::RequestResult(result) => {
                if let Some(pending) = self.state.pending.take() {
                    let _ = pending.reply.send(result.map_err(ClientError::Remote));
                }
                Ok(())
            }
            Event::Health { error } => {
                if self.state.error != error {
                    self.state.error = error;
                    self.publish();
                }
                Ok(())
            }
        }
    }

    async fn request(
        &mut self,
        transport: &mut ClientTransport,
        request: ClientCommand,
        reply: Reply,
    ) -> Result<(), ClientError> {
        if !self.state.initialized {
            let _ = reply.send(Err(ClientError::Disconnected));
            return Ok(());
        }
        let action = matches!(request, ClientCommand::Apply(_));
        if transport.send(request).await.is_err() {
            let _ = reply.send(Err(ClientError::Disconnected));
            return Err(ClientError::Disconnected);
        }
        self.state.pending = Some(Pending { reply, action });
        Ok(())
    }

    fn abandon_request(&mut self) {
        if let Some(pending) = self.state.pending.take() {
            let _ = pending.reply.send(Err(if pending.action {
                ClientError::OutcomeUnknown
            } else {
                ClientError::Disconnected
            }));
        }
    }

    #[cfg(not(feature = "websocket"))]
    async fn reconnect(&mut self, _url: &str) -> Option<ClientTransport> {
        None
    }

    #[cfg(feature = "websocket")]
    async fn reconnect(&mut self, url: &str) -> Option<ClientTransport> {
        let mut delay = 250;
        loop {
            let wait = delay;
            let connecting = async {
                platform::sleep(platform::reconnect_delay(wait)).await;
                ClientTransport::try_connect(url).await
            }
            .fuse();
            let command = self.commands.recv().fuse();
            futures_util::pin_mut!(connecting, command);
            futures_util::select! {
                next = connecting => match next {
                    Ok(next) => return Some(next),
                    Err(_) => delay = (delay * 2).min(5000),
                },
                command = command => match command {
                    Ok(Command::Request(_, reply)) => { let _ = reply.send(Err(ClientError::Disconnected)); }
                    Ok(Command::Close(reply)) => { self.closed = true; let _ = reply.send(Ok(())); return None; }
                    Err(_) => { self.closed = true; return None; }
                },
            }
        }
    }
}

fn is_terminal(result: &Result<(), ClientError>) -> bool {
    matches!(
        result,
        Err(ClientError::Protocol(_)
            | ClientError::Remote(RemoteError {
                code: RemoteErrorCode::UnsupportedVersion | RemoteErrorCode::Protocol,
                ..
            }))
    )
}
