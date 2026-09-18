use crate::{
    ClientError,
    protocol::{
        client::{ClientCommand, LocalClientTransport},
        server::ServerEvent,
    },
};

#[cfg(feature = "websocket")]
#[cfg_attr(not(target_arch = "wasm32"), path = "transport/native.rs")]
#[cfg_attr(target_arch = "wasm32", path = "transport/web.rs")]
mod ws;

pub(crate) enum ClientTransport {
    Local(LocalClientTransport),
    #[cfg(feature = "websocket")]
    WebSocket(Box<ws::WebSocketTransport>),
}

impl ClientTransport {
    pub async fn send(&mut self, command: ClientCommand) -> Result<(), ClientError> {
        match self {
            Self::Local(transport) => transport
                .send(command)
                .await
                .map_err(|_| ClientError::Disconnected),
            #[cfg(feature = "websocket")]
            Self::WebSocket(transport) => transport.send(command).await,
        }
    }

    pub async fn recv(&mut self) -> Result<ServerEvent, ClientError> {
        match self {
            Self::Local(transport) => transport
                .recv()
                .await
                .map_err(|_| ClientError::Disconnected),
            #[cfg(feature = "websocket")]
            Self::WebSocket(transport) => transport.recv().await,
        }
    }

    pub async fn close(self) {
        match self {
            Self::Local(transport) => transport.close(),
            #[cfg(feature = "websocket")]
            Self::WebSocket(transport) => transport.close().await,
        }
    }

    #[cfg(feature = "websocket")]
    pub async fn try_connect(url: &str) -> Result<Self, ClientError> {
        let transport = ws::WebSocketTransport::try_connect(url).await?;
        Ok(Self::WebSocket(Box::new(transport)))
    }
}
