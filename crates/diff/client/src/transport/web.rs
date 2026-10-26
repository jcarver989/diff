use crate::{
    ClientError, ConnectionHeader,
    error::transport,
    protocol::{client::ClientCommand, server::ServerMessage, shared::ProtocolError},
    transport::ClientMessageTransport,
};
use futures_util::{SinkExt, StreamExt};
use gloo_net::websocket::{Message, futures::WebSocket};

pub struct WebSocketTransport {
    socket: Option<WebSocket>,
}

impl WebSocketTransport {
    pub async fn try_connect(url: &str, headers: &[ConnectionHeader]) -> Result<Self, ClientError> {
        if !headers.is_empty() {
            return Err(ClientError::Transport(
                "custom WebSocket headers are unavailable in browser clients".to_owned(),
            ));
        }
        let socket = WebSocket::open(url).map_err(|error| {
            ClientError::Transport(format!(
                "{error}; HTTPS pages require a compatible wss:// URL"
            ))
        })?;
        Ok(Self {
            socket: Some(socket),
        })
    }

    fn socket(&mut self) -> Result<&mut WebSocket, ClientError> {
        self.socket.as_mut().ok_or(ClientError::Disconnected)
    }
}

impl ClientMessageTransport for WebSocketTransport {
    async fn send(&mut self, command: ClientCommand) -> Result<(), ClientError> {
        let message = Message::Text(command.encode()?);
        self.socket()?.send(message).await.map_err(transport)
    }

    async fn recv(&mut self) -> Result<ServerMessage, ClientError> {
        let message = self
            .socket()?
            .next()
            .await
            .ok_or(ClientError::Disconnected)?
            .map_err(transport)?;
        let Message::Text(text) = message else {
            return Err(ProtocolError::MessageType.into());
        };
        Ok(ServerMessage::decode(&text)?)
    }

    async fn close(&mut self) {
        if let Some(socket) = self.socket.take() {
            let _ = socket.close(None, None);
        }
    }
}
