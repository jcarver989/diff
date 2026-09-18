use crate::{
    ClientError, ConnectionHeader,
    error::transport,
    protocol::{
        client::{ClientCommand, DocumentCache},
        server::ServerEvent,
        shared::{MAX_MESSAGE_BYTES, ProtocolError},
    },
};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::{net::TcpStream, time::timeout};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_with_config,
    tungstenite::{Message, client::IntoClientRequest, protocol::WebSocketConfig},
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const WRITE_TIMEOUT: Duration = Duration::from_secs(60);

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct WebSocketTransport {
    socket: Socket,
    cache: DocumentCache,
}

impl WebSocketTransport {
    pub async fn try_connect(url: &str, headers: &[ConnectionHeader]) -> Result<Self, ClientError> {
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_MESSAGE_BYTES))
            .max_frame_size(Some(MAX_MESSAGE_BYTES));

        let mut request = url.into_client_request().map_err(transport)?;
        for header in headers {
            request
                .headers_mut()
                .append(header.name().clone(), header.value().clone());
        }

        let (socket, _) = timeout(
            CONNECT_TIMEOUT,
            connect_async_with_config(request, Some(config), false),
        )
        .await
        .map_err(transport)?
        .map_err(transport)?;

        Ok(Self {
            socket,
            cache: DocumentCache::default(),
        })
    }

    pub async fn send(&mut self, command: ClientCommand) -> Result<(), ClientError> {
        let message = Message::text(command.encode()?);
        timeout(WRITE_TIMEOUT, self.socket.send(message))
            .await
            .map_err(transport)?
            .map_err(transport)?;
        Ok(())
    }

    pub async fn recv(&mut self) -> Result<ServerEvent, ClientError> {
        loop {
            let message = self
                .socket
                .next()
                .await
                .ok_or(ClientError::Disconnected)?
                .map_err(transport)?;

            match message {
                Message::Close(_) => return Err(ClientError::Disconnected),
                Message::Ping(_) | Message::Pong(_) => {}
                Message::Text(text) => {
                    return Ok(self.cache.decode_event(&text)?);
                }
                _ => {
                    return Err(ProtocolError::MessageType.into());
                }
            }
        }
    }

    pub async fn close(mut self) {
        let _ = timeout(WRITE_TIMEOUT, self.socket.close(None)).await;
    }
}
