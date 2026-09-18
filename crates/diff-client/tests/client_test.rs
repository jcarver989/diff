use clankerdiff_client::{
    ClientError, ClientOptions, ConnectionState, DiffClient, DiffReviewEvent, DiffSnapshot,
    RepositoryAction,
    protocol::{
        client::ClientCommand,
        server::{LocalServerTransport, ServerEvent, local_transport_pair},
        shared::{Event, LIVE_PROTOCOL_VERSION},
    },
};
use clankerdiff_core::{DiffScope, Review, testing::DocumentBuilder};
use std::{error::Error, sync::Arc, time::Duration};
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(1);

#[tokio::test]
async fn subscriptions_retain_latest_state_without_lost_wakeups() -> Result<(), Box<dyn Error>> {
    let (client, server) = connected().await?;
    let mut subscription = client.subscribe();
    drop(client.subscribe());
    assert!(!client.state().capabilities.repository);

    server.send(document(DiffScope::Both)).await?;
    let state = subscription
        .wait_until(WAIT, |state| state.snapshot.is_some())
        .await?;
    assert!(matches!(state.connection, ConnectionState::Connected));
    assert!(state.capabilities.repository && state.capabilities.submit);
    assert!(state.status().is_none());
    let mut installed = None;
    assert!(state.snapshot_if_changed(&mut installed).is_some());
    assert!(state.snapshot_if_changed(&mut installed).is_none());

    client.close().await?;
    timeout(WAIT, async { while server.recv().await.is_ok() {} }).await?;
    Ok(())
}

#[tokio::test]
async fn sent_action_is_not_replayed_and_reports_unknown_outcome() -> Result<(), Box<dyn Error>> {
    let (client, server) = connected().await?;
    let acting = client.clone();
    let request = tokio::spawn(async move { acting.apply(RepositoryAction::StageAll).await });
    assert!(matches!(server.recv().await?, ClientCommand::Apply(_)));
    drop(server);
    assert!(matches!(request.await?, Err(ClientError::OutcomeUnknown)));
    Ok(())
}

#[tokio::test]
async fn terminal_events_wait_for_acknowledgement() -> Result<(), Box<dyn Error>> {
    for event in [
        DiffReviewEvent::SubmitReview(Review::default().submission()),
        DiffReviewEvent::Cancel,
    ] {
        let (client, server) = connected().await?;
        let handling = client.clone();
        let request = tokio::spawn(async move { handling.handle(event).await });
        assert!(matches!(
            server.recv().await?,
            ClientCommand::Submit(_) | ClientCommand::Cancel
        ));
        assert!(!request.is_finished());
        server.send(Event::RequestResult(Ok(()))).await?;
        timeout(WAIT, request).await???;
        client.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn sent_submission_reports_unknown_outcome_after_disconnect() -> Result<(), Box<dyn Error>> {
    let (client, server) = connected().await?;
    let submitting = client.clone();
    let request = tokio::spawn(async move {
        submitting
            .handle(DiffReviewEvent::SubmitReview(
                Review::default().submission(),
            ))
            .await
    });
    assert!(matches!(server.recv().await?, ClientCommand::Submit(_)));
    drop(server);
    assert!(matches!(request.await?, Err(ClientError::OutcomeUnknown)));
    Ok(())
}

#[tokio::test]
async fn one_operation_runs_at_a_time_and_scope_settles_with_its_document()
-> Result<(), Box<dyn Error>> {
    let (client, server) = connected().await?;
    server.send(document(DiffScope::Both)).await?;
    let switching = client.clone();
    let request = tokio::spawn(async move {
        switching
            .handle(DiffReviewEvent::SetScope(DiffScope::Staged))
            .await
    });
    assert!(matches!(
        server.recv().await?,
        ClientCommand::SetScope(DiffScope::Staged)
    ));
    let refreshing = client.clone();
    let refresh = tokio::spawn(async move { refreshing.refresh().await });
    assert!(!request.is_finished());
    assert!(!refresh.is_finished());

    server.send(document(DiffScope::Staged)).await?;
    server.send(Event::RequestResult(Ok(()))).await?;
    timeout(WAIT, request).await???;
    assert!(matches!(server.recv().await?, ClientCommand::Refresh));
    server.send(Event::RequestResult(Ok(()))).await?;
    timeout(WAIT, refresh).await???;
    assert_eq!(
        client.state().snapshot.as_ref().ok_or("snapshot")?.scope,
        DiffScope::Staged
    );
    client.close().await?;
    Ok(())
}

#[tokio::test]
async fn dropping_the_last_handle_stops_the_transport() -> Result<(), Box<dyn Error>> {
    let (client, server) = connected().await?;
    let mut subscription = client.subscribe();
    drop(client);
    timeout(WAIT, async { while server.recv().await.is_ok() {} }).await?;
    timeout(WAIT, async {
        while subscription.changed().await.is_ok() {}
    })
    .await?;
    Ok(())
}

fn document(scope: DiffScope) -> ServerEvent {
    Event::Document(Arc::new(DiffSnapshot {
        scope,
        document: DocumentBuilder::new().changed("a", "old", "new").build(),
    }))
}

async fn connected() -> Result<(DiffClient, LocalServerTransport), Box<dyn Error>> {
    let (transport, server) = local_transport_pair(4);
    let connecting = tokio::spawn(DiffClient::from_transport(
        transport,
        ClientOptions::default(),
    ));
    assert!(matches!(
        server.recv().await?,
        ClientCommand::Initialize { .. }
    ));
    server
        .send(Event::Initialize {
            protocol_version: LIVE_PROTOCOL_VERSION,
            repository_root: "/remote".to_owned(),
        })
        .await?;
    Ok((connecting.await??, server))
}
