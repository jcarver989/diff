use crate::{
    ClientError,
    protocol::{
        client::{ClientCommand, DocumentCache, LocalClientTransport},
        server::{ServerEvent, ServerMessage},
        shared::LocalEnd,
    },
};
use std::future::Future;

#[cfg(feature = "websocket")]
use crate::ConnectionHeader;

#[cfg(feature = "websocket")]
#[cfg_attr(not(target_arch = "wasm32"), path = "transport/native.rs")]
#[cfg_attr(target_arch = "wasm32", path = "transport/web.rs")]
mod ws;

#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send> MaybeSend for T {}
#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T> MaybeSend for T {}

pub trait ClientMessageTransport: MaybeSend + 'static {
    fn send(
        &mut self,
        command: ClientCommand,
    ) -> impl Future<Output = Result<(), ClientError>> + MaybeSend;
    fn recv(&mut self) -> impl Future<Output = Result<ServerMessage, ClientError>> + MaybeSend;
    fn close(&mut self) -> impl Future<Output = ()> + MaybeSend;
}

pub(crate) trait Transport: MaybeSend + 'static {
    fn send(
        &mut self,
        command: ClientCommand,
    ) -> impl Future<Output = Result<(), ClientError>> + MaybeSend;
    fn recv(&mut self) -> impl Future<Output = Result<ServerEvent, ClientError>> + MaybeSend;
    fn close(&mut self) -> impl Future<Output = ()> + MaybeSend;

    fn reconnects(&self) -> bool {
        false
    }

    fn reconnect(&mut self) -> impl Future<Output = Result<(), ClientError>> + MaybeSend {
        async { Err(ClientError::Disconnected) }
    }
}

impl Transport for LocalClientTransport {
    async fn send(&mut self, command: ClientCommand) -> Result<(), ClientError> {
        LocalEnd::send(self, command)
            .await
            .map_err(|_| ClientError::Disconnected)
    }

    async fn recv(&mut self) -> Result<ServerEvent, ClientError> {
        LocalEnd::recv(self)
            .await
            .map_err(|_| ClientError::Disconnected)
    }

    async fn close(&mut self) {
        LocalEnd::close(self);
    }
}

impl ClientMessageTransport for LocalEnd<ClientCommand, ServerMessage> {
    async fn send(&mut self, command: ClientCommand) -> Result<(), ClientError> {
        LocalEnd::send(self, command)
            .await
            .map_err(|_| ClientError::Disconnected)
    }

    async fn recv(&mut self) -> Result<ServerMessage, ClientError> {
        LocalEnd::recv(self)
            .await
            .map_err(|_| ClientError::Disconnected)
    }

    async fn close(&mut self) {
        LocalEnd::close(self);
    }
}

pub(crate) struct Decoded<T> {
    transport: T,
    cache: DocumentCache,
}

impl<T> Decoded<T> {
    pub(crate) fn new(transport: T) -> Self {
        Self {
            transport,
            cache: DocumentCache::default(),
        }
    }
}

impl<T: ClientMessageTransport> Transport for Decoded<T> {
    async fn send(&mut self, command: ClientCommand) -> Result<(), ClientError> {
        self.transport.send(command).await
    }

    async fn recv(&mut self) -> Result<ServerEvent, ClientError> {
        let message = self.transport.recv().await?;
        Ok(self.cache.apply_event(message)?)
    }

    async fn close(&mut self) {
        self.transport.close().await;
    }
}

#[cfg(feature = "websocket")]
pub(crate) struct Reconnecting {
    url: String,
    headers: Vec<ConnectionHeader>,
    socket: Decoded<ws::WebSocketTransport>,
}

#[cfg(feature = "websocket")]
impl Reconnecting {
    pub(crate) async fn connect(
        url: &str,
        headers: Vec<ConnectionHeader>,
    ) -> Result<Self, ClientError> {
        let socket = ws::WebSocketTransport::try_connect(url, &headers).await?;
        Ok(Self {
            url: url.to_owned(),
            headers,
            socket: Decoded::new(socket),
        })
    }
}

#[cfg(feature = "websocket")]
impl Transport for Reconnecting {
    async fn send(&mut self, command: ClientCommand) -> Result<(), ClientError> {
        self.socket.send(command).await
    }

    async fn recv(&mut self) -> Result<ServerEvent, ClientError> {
        self.socket.recv().await
    }

    async fn close(&mut self) {
        self.socket.close().await;
    }

    fn reconnects(&self) -> bool {
        true
    }

    async fn reconnect(&mut self) -> Result<(), ClientError> {
        let socket = ws::WebSocketTransport::try_connect(&self.url, &self.headers).await?;
        self.socket = Decoded::new(socket);
        Ok(())
    }
}
