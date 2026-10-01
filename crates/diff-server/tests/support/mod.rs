#![allow(dead_code)]

use async_channel::{Receiver, Sender, bounded};
use clankerdiff_client::{
    ClientError, ClientMessageTransport, ClientSubscription,
    protocol::{client::ClientCommand, server::ServerMessage},
};
#[cfg(feature = "websocket")]
use clankerdiff_client::{ClientOptions, DiffClient};
use clankerdiff_git::testing::{RepoFixture, RepoFixtureBuilder};
use clankerdiff_protocol::shared::{RemoteError, RemoteErrorCode};
#[cfg(feature = "websocket")]
use clankerdiff_server::ServerListener;
use clankerdiff_server::{DiffServer, ServerMessageTransport, ServerOptions};
use std::{error::Error, time::Duration};

pub const WAIT: Duration = Duration::from_secs(10);

/// A committed one-file repository with a backend already serving it.
pub struct TestServer {
    pub repo: RepoFixture,
    pub server: DiffServer,
}

impl TestServer {
    pub async fn start() -> Result<Self, Box<dyn Error>> {
        let repo = RepoFixtureBuilder::new()
            .file("a", "old\n")
            .committed()
            .build();
        let server =
            DiffServer::open(repo.repository().await.root(), ServerOptions::default()).await?;
        Ok(Self { repo, server })
    }

    #[cfg(feature = "websocket")]
    pub async fn listen(&self) -> Result<(ServerListener, String), Box<dyn Error>> {
        let listener = self.server.listen("127.0.0.1:0".parse()?).await?;
        let url = format!("ws://{}/ws", listener.local_addr());
        Ok((listener, url))
    }

    #[cfg(feature = "websocket")]
    pub async fn connect_ws(
        &self,
    ) -> Result<(ServerListener, DiffClient, ClientSubscription), Box<dyn Error>> {
        let (listener, url) = self.listen().await?;
        let client = DiffClient::connect(&url, ClientOptions::default()).await?;
        let mut subscription = client.subscribe();
        subscription
            .wait_until(WAIT, |state| {
                state
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.document.files.is_empty())
            })
            .await?;
        Ok((listener, client, subscription))
    }
}

pub async fn wait_for_text(
    subscription: &mut ClientSubscription,
    expected: &str,
) -> Result<(), Box<dyn Error>> {
    subscription
        .wait_until(WAIT, |state| {
            state.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.document.files.first().is_some_and(|file| {
                    file.new_source
                        .as_ref()
                        .is_ok_and(|source| source.text() == expected)
                })
            })
        })
        .await?;
    Ok(())
}

pub struct Channel<Out, In> {
    tx: Sender<Out>,
    rx: Receiver<In>,
}

impl ClientMessageTransport for Channel<ClientCommand, ServerMessage> {
    async fn send(&mut self, command: ClientCommand) -> Result<(), ClientError> {
        self.tx
            .send(command)
            .await
            .map_err(|_| ClientError::Disconnected)
    }

    async fn recv(&mut self) -> Result<ServerMessage, ClientError> {
        self.rx.recv().await.map_err(|_| ClientError::Disconnected)
    }

    async fn close(&mut self) {
        self.tx.close();
        self.rx.close();
    }
}

impl ServerMessageTransport for Channel<ServerMessage, ClientCommand> {
    async fn recv(&mut self) -> Result<ClientCommand, RemoteError> {
        self.rx.recv().await.map_err(|_| closed())
    }

    async fn send(&mut self, message: ServerMessage) -> Result<(), RemoteError> {
        self.tx.send(message).await.map_err(|_| closed())
    }

    async fn close(&mut self) {
        self.tx.close();
        self.rx.close();
    }
}

pub fn message_transports() -> (
    Channel<ClientCommand, ServerMessage>,
    Channel<ServerMessage, ClientCommand>,
) {
    let (commands_tx, commands_rx) = bounded(4);
    let (messages_tx, messages_rx) = bounded(4);
    (
        Channel {
            tx: commands_tx,
            rx: messages_rx,
        },
        Channel {
            tx: messages_tx,
            rx: commands_rx,
        },
    )
}

fn closed() -> RemoteError {
    RemoteError::new(RemoteErrorCode::Cancelled, "channel closed")
}
