#![cfg(feature = "remote")]

use clankerdiff_client::{ClientOptions, DiffClient, DiffScope};
use clankerdiff_git::testing::RepoFixtureBuilder;
use clankerdiff_server::{DiffServer, ServerOptions};
use std::{error::Error, time::Duration};

#[tokio::test]
async fn renderer_independent_remote_api() -> Result<(), Box<dyn Error>> {
    let repo = RepoFixtureBuilder::new()
        .file("file", "old\n")
        .committed()
        .build();
    repo.write("file", "new\n");
    let server = DiffServer::open(repo.repository().await.root(), ServerOptions::default()).await?;
    let listener = server.listen("127.0.0.1:0".parse()?).await?;
    let client = DiffClient::connect(
        &format!("ws://{}/ws", listener.local_addr()),
        ClientOptions::default(),
    )
    .await?;
    let state = client
        .subscribe()
        .wait_until(Duration::from_secs(10), |state| state.snapshot.is_some())
        .await?;
    assert_eq!(
        state
            .snapshot
            .as_ref()
            .ok_or("snapshot")?
            .document
            .files
            .len(),
        1
    );
    client.set_scope(DiffScope::Staged).await?;
    assert!(
        client
            .state()
            .snapshot
            .as_ref()
            .ok_or("snapshot")?
            .document
            .files
            .is_empty()
    );
    client.close().await?;
    listener.shutdown().await?;
    server.shutdown().await?;
    Ok(())
}
