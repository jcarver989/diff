use crate::server::{protocol, stopped};
use clankerdiff_protocol::{
    client::ClientCommand,
    server::{LocalServerMessageTransport, LocalServerTransport, SentDocument, ServerEvent, ServerMessage},
    shared::{LocalEnd, RemoteError, StreamError, StreamTransport},
};
use std::future::Future;
use tokio::io::{AsyncRead, AsyncWrite};

pub trait ServerMessageTransport: Send + 'static {
    fn recv(&mut self) -> impl Future<Output = Result<ClientCommand, RemoteError>> + Send;
    fn send(&mut self, message: ServerMessage) -> impl Future<Output = Result<(), RemoteError>> + Send;
    fn close(&mut self) -> impl Future<Output = ()> + Send;
}

pub(crate) trait Transport: Send + 'static {
    fn recv(&mut self) -> impl Future<Output = Result<ClientCommand, RemoteError>> + Send;
    fn send(&mut self, event: ServerEvent) -> impl Future<Output = Result<(), RemoteError>> + Send;
    fn close(&mut self) -> impl Future<Output = ()> + Send;
}

impl Transport for LocalServerTransport {
    async fn recv(&mut self) -> Result<ClientCommand, RemoteError> {
        LocalEnd::recv(self).await.map_err(|_| stopped())
    }

    async fn send(&mut self, event: ServerEvent) -> Result<(), RemoteError> {
        LocalEnd::send(self, event).await.map_err(|_| stopped())
    }

    async fn close(&mut self) {
        LocalEnd::close(self);
    }
}

impl ServerMessageTransport for LocalServerMessageTransport {
    async fn recv(&mut self) -> Result<ClientCommand, RemoteError> {
        LocalEnd::recv(self).await.map_err(|_| stopped())
    }

    async fn send(&mut self, message: ServerMessage) -> Result<(), RemoteError> {
        LocalEnd::send(self, message).await.map_err(|_| stopped())
    }

    async fn close(&mut self) {
        LocalEnd::close(self);
    }
}

impl<T, U> ServerMessageTransport for StreamTransport<T, U>
where
    T: AsyncRead + Unpin + Send + 'static,
    U: AsyncWrite + Unpin + Send + 'static,
{
    async fn recv(&mut self) -> Result<ClientCommand, RemoteError> {
        let line = self.read_line().await.map_err(|error| match error {
            StreamError::Closed => stopped(),
            error => protocol(error),
        })?;
        Ok(ClientCommand::decode(&line)?)
    }

    async fn send(&mut self, message: ServerMessage) -> Result<(), RemoteError> {
        self.write_line(&message.encode()?).await.map_err(|_| stopped())
    }

    async fn close(&mut self) {
        self.shutdown().await;
    }
}

pub(crate) struct Encoded<T> {
    transport: T,
    sent: SentDocument,
}

impl<T> Encoded<T> {
    pub(crate) fn new(transport: T) -> Self {
        Self { transport, sent: SentDocument::default() }
    }
}

impl<T: ServerMessageTransport> Transport for Encoded<T> {
    async fn recv(&mut self) -> Result<ClientCommand, RemoteError> {
        self.transport.recv().await
    }

    async fn send(&mut self, event: ServerEvent) -> Result<(), RemoteError> {
        self.transport.send(self.sent.encode_event(event)).await
    }

    async fn close(&mut self) {
        self.transport.close().await;
    }
}
