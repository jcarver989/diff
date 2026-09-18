#![allow(dead_code)]

use clankerdiff_client::{ClientOptions, ClientSubscription, DiffClient};
use clankerdiff_git::testing::{RepoFixture, RepoFixtureBuilder};
use clankerdiff_server::{DiffServer, ServerListener, ServerOptions};
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

    pub async fn listen(&self) -> Result<(ServerListener, String), Box<dyn Error>> {
        let listener = self.server.listen("127.0.0.1:0".parse()?).await?;
        let url = format!("ws://{}/ws", listener.local_addr());
        Ok((listener, url))
    }

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
