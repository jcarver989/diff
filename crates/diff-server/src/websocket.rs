use crate::{
    ServerError,
    server::{Handle, protocol, stopped},
    transport::{Encoded, ServerMessageTransport},
};
use axum::{
    Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
    routing::get,
};
use clankerdiff_protocol::{
    client::ClientCommand,
    server::ServerMessage,
    shared::{MAX_MESSAGE_BYTES, ProtocolError, RemoteError},
};
use futures_util::{SinkExt, StreamExt};
use std::{future::IntoFuture, net::SocketAddr};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;

pub struct ServerListener {
    address: SocketAddr,
    stop: CancellationToken,
    task: JoinHandle<Result<(), std::io::Error>>,
}

impl ServerListener {
    #[must_use]
    pub const fn local_addr(&self) -> SocketAddr {
        self.address
    }

    pub async fn shutdown(mut self) -> Result<(), ServerError> {
        self.stop.cancel();
        (&mut self.task).await.map_err(|_| ServerError::Stopped)??;
        Ok(())
    }
}

impl Drop for ServerListener {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

pub(crate) async fn listen(
    handle: Handle,
    address: SocketAddr,
) -> Result<ServerListener, ServerError> {
    let listener = TcpListener::bind(address).await?;
    let address = listener.local_addr()?;
    let stop = handle.child_token();
    let router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/ws", get(upgrade))
        .with_state(Accepting {
            handle: handle.clone(),
            stop: stop.clone(),
        });
    let shutdown = stop.clone();
    let task = handle.spawn(
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .into_future(),
    );
    Ok(ServerListener {
        address,
        stop,
        task,
    })
}

#[derive(Clone)]
struct Accepting {
    handle: Handle,
    stop: CancellationToken,
}

async fn upgrade(State(accepting): State<Accepting>, ws: WebSocketUpgrade) -> Response {
    ws.max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| async move {
            accepting.handle.accept(
                Encoded::new(WebSocketTransport::new(socket)),
                accepting.stop,
            );
        })
}

pub(crate) struct WebSocketTransport {
    socket: WebSocket,
}

impl WebSocketTransport {
    pub fn new(socket: WebSocket) -> Self {
        Self { socket }
    }
}

impl ServerMessageTransport for WebSocketTransport {
    async fn recv(&mut self) -> Result<ClientCommand, RemoteError> {
        loop {
            let message = self.socket.next().await.ok_or_else(stopped)?;
            let message = message.map_err(protocol)?;
            match message {
                Message::Close(_) => return Err(stopped()),
                Message::Ping(_) | Message::Pong(_) => {}
                Message::Text(text) => return Ok(ClientCommand::decode(&text)?),
                Message::Binary(_) => return Err(ProtocolError::MessageType.into()),
            }
        }
    }

    async fn send(&mut self, message: ServerMessage) -> Result<(), RemoteError> {
        self.socket
            .send(Message::text(message.encode()?))
            .await
            .map_err(protocol)
    }

    async fn close(&mut self) {
        let _ = self.socket.close().await;
    }
}
