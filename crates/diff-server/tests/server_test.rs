mod support;

use clankerdiff_client::{
    ClientOptions, ClientSubscription, ConnectionState, DiffClient, DiffScope, RepositoryAction,
};
use std::{
    error::Error,
    sync::Arc,
    time::{Duration, Instant},
};
use support::{TestServer, WAIT, wait_for_text};
use tokio::time::timeout;

#[tokio::test]
async fn local_clients_have_independent_scopes_and_observe_actions() -> Result<(), Box<dyn Error>> {
    let fixture = TestServer::start().await?;
    fixture.repo.write("a", "new\n");
    let a = DiffClient::from_transport(fixture.server.connect()?, ClientOptions::default()).await?;
    let b = DiffClient::from_transport(fixture.server.connect()?, ClientOptions::default()).await?;
    let mut state_a = a.subscribe();
    let mut state_b = b.subscribe();
    wait_for(&mut state_a, DiffScope::Both, 1).await?;
    wait_for(&mut state_b, DiffScope::Both, 1).await?;
    timeout(WAIT, a.set_scope(DiffScope::Staged)).await??;
    assert_eq!(
        a.state().snapshot.as_ref().ok_or("snapshot")?.scope,
        DiffScope::Staged
    );
    assert_eq!(
        b.state().snapshot.as_ref().ok_or("snapshot")?.scope,
        DiffScope::Both
    );
    a.apply(RepositoryAction::StageAll).await?;
    wait_for(&mut state_a, DiffScope::Staged, 1).await?;
    a.close().await?;
    b.refresh().await?;
    b.close().await?;
    fixture.server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn websocket_client_bootstraps_and_tracks_real_edits() -> Result<(), Box<dyn Error>> {
    let fixture = TestServer::start().await?;
    let (listener, client, mut state) = fixture.connect_ws().await?;
    fixture.repo.write("a", "new\n");
    wait_for(&mut state, DiffScope::Both, 1).await?;
    assert_eq!(
        state
            .latest()
            .snapshot
            .as_ref()
            .ok_or("snapshot")?
            .document
            .files[0]
            .new_source
            .as_ref()
            .map_err(Clone::clone)?
            .text(),
        "new\n"
    );
    client.close().await?;
    listener.shutdown().await?;
    fixture.server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn listener_shutdown_disconnects_and_client_reconnects() -> Result<(), Box<dyn Error>> {
    let fixture = TestServer::start().await?;
    let (listener, client, mut state) = fixture.connect_ws().await?;
    let address = listener.local_addr();
    let previous = client.state().snapshot.clone().ok_or("snapshot")?;
    timeout(Duration::from_secs(2), listener.shutdown()).await??;
    state
        .wait_until(Duration::from_secs(2), |state| {
            matches!(state.connection, ConnectionState::Connecting) && state.snapshot.is_some()
        })
        .await?;
    assert!(Arc::ptr_eq(
        &previous,
        client
            .state()
            .snapshot
            .as_ref()
            .ok_or("retained snapshot")?
    ));
    fixture.repo.write("a", "after disconnect\n");
    let listener = fixture.server.listen(address).await?;
    wait_for(&mut state, DiffScope::Both, 1).await?;
    client.close().await?;
    listener.shutdown().await?;
    fixture.server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn server_shutdown_closes_active_websocket_connections() -> Result<(), Box<dyn Error>> {
    let fixture = TestServer::start().await?;
    let (listener, client, mut state) = fixture.connect_ws().await?;

    timeout(Duration::from_secs(2), fixture.server.shutdown()).await??;
    state
        .wait_until(Duration::from_secs(2), |state| {
            !matches!(state.connection, ConnectionState::Connected)
        })
        .await?;
    client.close().await?;
    listener.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn continuous_writes_publish_before_the_writer_stops() -> Result<(), Box<dyn Error>> {
    let fixture = TestServer::start().await?;
    let client =
        DiffClient::from_transport(fixture.server.connect()?, ClientOptions::default()).await?;
    let mut state = client.subscribe();
    wait_for(&mut state, DiffScope::Both, 0).await?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut revision = 0;
    let observed = loop {
        if Instant::now() >= deadline {
            break false;
        }
        revision += 1;
        fixture.repo.write("a", format!("revision {revision}\n"));
        tokio::time::sleep(Duration::from_millis(25)).await;
        if state
            .latest()
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| !snapshot.document.files.is_empty())
        {
            break true;
        }
    };
    assert!(
        observed,
        "fixed-window debounce must publish while writes continue"
    );

    revision += 1;
    let settled = format!("revision {revision}\n");
    fixture.repo.write("a", settled.clone());
    wait_for_text(&mut state, &settled).await?;
    client.close().await?;
    fixture.server.shutdown().await?;
    Ok(())
}

async fn wait_for(
    state: &mut ClientSubscription,
    scope: DiffScope,
    files: usize,
) -> Result<(), Box<dyn Error>> {
    state
        .wait_until(WAIT, |state| {
            state.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.scope == scope && snapshot.document.files.len() == files
            })
        })
        .await?;
    Ok(())
}
