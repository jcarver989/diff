use crate::{
    ClientError,
    error::transport,
    protocol::{
        client::{ClientCommand, DocumentCache},
        server::ServerEvent,
        shared::ProtocolError,
    },
};
use futures_util::{SinkExt, StreamExt};
use gloo_net::websocket::{Message, futures::WebSocket};
pub struct WebSocketTransport {
    socket: WebSocket,
    cache: DocumentCache,
}

impl WebSocketTransport {
    pub async fn try_connect(url: &str) -> Result<Self, ClientError> {
        let socket = WebSocket::open(url).map_err(|error| {
            ClientError::Transport(format!(
                "{error}; HTTPS pages require a compatible wss:// URL"
            ))
        })?;
        Ok(Self {
            socket,
            cache: DocumentCache::default(),
        })
    }

    pub async fn send(&mut self, command: ClientCommand) -> Result<(), ClientError> {
        self.socket
            .send(Message::Text(command.encode()?))
            .await
            .map_err(transport)
    }

    pub async fn recv(&mut self) -> Result<ServerEvent, ClientError> {
        let message = self
            .socket
            .next()
            .await
            .ok_or(ClientError::Disconnected)?
            .map_err(transport)?;
        let Message::Text(text) = message else {
            return Err(ProtocolError::MessageType.into());
        };
        Ok(self.cache.decode_event(&text)?)
    }

    pub async fn close(self) {
        let _ = self.socket.close(None, None);
    }
}
