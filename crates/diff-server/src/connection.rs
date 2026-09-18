use crate::{
    server::{Context, protocol, stopped},
    websocket::WebSocketTransport,
};
use clankerdiff_core::DiffScope;
use clankerdiff_git::RepositorySnapshot;
use clankerdiff_protocol::{
    client::ClientCommand,
    server::{LocalServerTransport, ServerEvent},
    shared::{DiffSnapshot, Event, LIVE_PROTOCOL_VERSION, RemoteError, RemoteErrorCode},
};
use clankerdiff_watch::RepositoryState;
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};
use tokio::{
    sync::watch,
    time::{MissedTickBehavior, interval, timeout},
};
use tokio_util::sync::CancellationToken;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const HEARTBEAT: Duration = Duration::from_secs(15);
const SEND_TIMEOUT: Duration = Duration::from_secs(60);

type Command = Pin<Box<dyn Future<Output = CommandResult> + Send>>;

struct CommandResult {
    result: Result<(), RemoteError>,
    state: Option<watch::Receiver<RepositoryState>>,
}

pub(crate) enum ServerTransport {
    Local(LocalServerTransport),
    WebSocket(Box<WebSocketTransport>),
}

impl ServerTransport {
    async fn recv(&mut self) -> Result<ClientCommand, RemoteError> {
        match self {
            Self::Local(transport) => transport.recv().await.map_err(|_| stopped()),
            Self::WebSocket(transport) => transport.recv().await,
        }
    }

    async fn send(&mut self, event: ServerEvent) -> Result<(), RemoteError> {
        match self {
            Self::Local(transport) => transport.send(event).await.map_err(|_| stopped()),
            Self::WebSocket(transport) => transport.send(event).await,
        }
    }

    async fn close(&mut self) {
        match self {
            Self::Local(transport) => transport.close(),
            Self::WebSocket(transport) => transport.close().await,
        }
    }
}

pub(crate) async fn accept(
    context: Arc<Context>,
    mut transport: ServerTransport,
    stop: CancellationToken,
) {
    let result = tokio::select! {
        result = session(&context, &mut transport) => Some(result),
        () = stop.cancelled() => None,
    };
    if let Some(Err(error)) = result {
        let _ = publish(&mut transport, Event::Error(error)).await;
    }
    transport.close().await;
}

async fn session(context: &Context, transport: &mut ServerTransport) -> Result<(), RemoteError> {
    let scope = handshake(context, transport).await?;
    let state = context
        .watcher
        .subscribe(scope)
        .await
        .map_err(watcher_error)?;
    serve(context, transport, state, scope).await
}

async fn handshake(
    context: &Context,
    transport: &mut ServerTransport,
) -> Result<DiffScope, RemoteError> {
    let hello = timeout(HANDSHAKE_TIMEOUT, transport.recv())
        .await
        .map_err(|_| stopped())??;
    let ClientCommand::Initialize {
        protocol_version,
        scope,
    } = hello
    else {
        return Err(protocol("expected Hello"));
    };
    if protocol_version != LIVE_PROTOCOL_VERSION {
        return Err(RemoteError::new(
            RemoteErrorCode::UnsupportedVersion,
            "unsupported live protocol version",
        ));
    }
    publish(
        transport,
        Event::Initialize {
            protocol_version: LIVE_PROTOCOL_VERSION,
            repository_root: context.root.to_string(),
        },
    )
    .await?;
    Ok(scope)
}

async fn serve(
    context: &Context,
    transport: &mut ServerTransport,
    mut state: watch::Receiver<RepositoryState>,
    mut scope: DiffScope,
) -> Result<(), RemoteError> {
    let mut published: Option<Arc<RepositorySnapshot>> = None;
    let mut health: Option<RemoteError> = None;
    let mut running: Option<Command> = None;
    let mut finished: Option<Result<(), RemoteError>> = None;
    let mut heartbeat = interval(HEARTBEAT);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    heartbeat.tick().await;

    loop {
        let current = state.borrow_and_update().clone();
        if published
            .as_ref()
            .is_none_or(|old| !Arc::ptr_eq(old, &current.snapshot))
        {
            publish(
                transport,
                Event::Document(Arc::new(DiffSnapshot {
                    scope: current.snapshot.scope,
                    document: current.snapshot.document.clone(),
                })),
            )
            .await?;
            published = Some(current.snapshot.clone());
        }
        let latest = current.error.as_ref().map(watcher_error);
        if latest != health {
            health = latest.clone();
            publish(transport, Event::Health { error: latest }).await?;
        }
        if let Some(result) = finished.take() {
            publish(transport, Event::RequestResult(result)).await?;
        }
        tokio::select! {
            changed = state.changed() => if changed.is_err() { return Err(stopped()); },
            command = transport.recv(), if running.is_none() => {
                running = Some(start(context, scope, command?)?);
            }
            result = async {
                match running.as_mut() {
                    Some(command) => command.await,
                    None => std::future::pending().await,
                }
            } => {
                if let Some(next) = result.state {
                    scope = next.borrow().snapshot.scope;
                    state = next;
                    published = None;
                    health = None;
                }
                finished = Some(result.result);
                running = None;
            }
            _ = heartbeat.tick() => {
                publish(transport, Event::Health { error: health.clone() }).await?;
            }
        }
    }
}

fn start(
    context: &Context,
    current_scope: DiffScope,
    command: ClientCommand,
) -> Result<Command, RemoteError> {
    match command {
        ClientCommand::Initialize { .. } => Err(protocol("repeated Hello")),
        ClientCommand::SetScope(scope) => {
            let watcher = context.watcher.clone();
            Ok(Box::pin(async move {
                match watcher.subscribe(scope).await {
                    Ok(state) => CommandResult {
                        result: Ok(()),
                        state: Some(state),
                    },
                    Err(error) => CommandResult {
                        result: Err(watcher_error(error)),
                        state: None,
                    },
                }
            }))
        }
        ClientCommand::Refresh => {
            let watcher = context.watcher.clone();
            Ok(Box::pin(async move {
                CommandResult {
                    result: watcher.refresh(current_scope).await.map_err(watcher_error),
                    state: None,
                }
            }))
        }
        ClientCommand::Apply(action) => {
            let repository = context.repository.clone();
            let watcher = context.watcher.clone();
            Ok(Box::pin(async move {
                let result = async {
                    repository.apply(action).await.map_err(|error| {
                        RemoteError::new(RemoteErrorCode::Git, error.to_string())
                    })?;
                    let _ = watcher.refresh(current_scope).await;
                    Ok(())
                }
                .await;
                CommandResult {
                    result,
                    state: None,
                }
            }))
        }
    }
}

async fn publish(transport: &mut ServerTransport, event: ServerEvent) -> Result<(), RemoteError> {
    timeout(SEND_TIMEOUT, transport.send(event))
        .await
        .map_err(|_| stopped())?
}

fn watcher_error(error: impl ToString) -> RemoteError {
    let message = error.to_string();
    drop(error);
    RemoteError::new(RemoteErrorCode::Watcher, message)
}
