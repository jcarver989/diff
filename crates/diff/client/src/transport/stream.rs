use crate::{
    ClientError,
    protocol::{
        client::ClientCommand,
        server::ServerMessage,
        shared::{StreamError, StreamTransport},
    },
    transport::ClientMessageTransport,
};
use tokio::io::{AsyncRead, AsyncWrite};

impl<T, U> ClientMessageTransport for StreamTransport<T, U>
where
    T: AsyncRead + Unpin + Send + 'static,
    U: AsyncWrite + Unpin + Send + 'static,
{
    async fn send(&mut self, command: ClientCommand) -> Result<(), ClientError> {
        self.write_line(&command.encode()?).await.map_err(|_| ClientError::Disconnected)
    }

    async fn recv(&mut self) -> Result<ServerMessage, ClientError> {
        let line = self.read_line().await.map_err(|error| match error {
            StreamError::Closed => ClientError::Disconnected,
            StreamError::Io(error) => ClientError::Transport(error.to_string()),
            StreamError::Protocol(error) => error.into(),
        })?;
        Ok(ServerMessage::decode(&line)?)
    }

    async fn close(&mut self) {
        self.shutdown().await;
    }
}
