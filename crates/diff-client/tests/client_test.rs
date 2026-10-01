use async_channel::{Receiver, Sender, bounded};
use clankerdiff_client::{
    ClientError, ClientMessageTransport, ClientOptions, ConnectionState, DiffClient,
    DiffReviewEvent, DiffSnapshot, RepositoryAction,
    protocol::{
        client::ClientCommand,
        server::{LocalServerTransport, ServerEvent, ServerMessage, local_transport_pair},
        shared::{DocumentUpdate, Event, FileEntry, LIVE_PROTOCOL_VERSION},
    },
};
use clankerdiff_core::{DiffScope, FileDiff, Review, testing::DocumentBuilder};
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

#[tokio::test]
async fn message_transports_rebuild_documents_and_fail_on_unknown_files()
-> Result<(), Box<dyn Error>> {
    let (transport, commands, messages) = message_channel();
    let client = DiffClient::spawn(transport, ClientOptions::default());
    assert!(matches!(
        commands.recv().await?,
        ClientCommand::Initialize { .. }
    ));
    messages
        .send(Event::Initialize {
            protocol_version: LIVE_PROTOCOL_VERSION,
            repository_root: "/remote".to_owned(),
        })
        .await?;
    let mut updates = client.subscribe();

    let file = Arc::new(FileDiff::from_texts("a", "old", "new")?);
    messages
        .send(Event::Document(update(vec![FileEntry::Changed(file)])))
        .await?;
    let first = updates
        .wait_until(WAIT, |state| state.snapshot.is_some())
        .await?;
    let installed = first.snapshot.clone().ok_or("snapshot")?;
    assert_eq!(installed.document.files[0].path.as_str(), "a");

    messages
        .send(Event::Document(update(vec![FileEntry::Unchanged(
            "a".try_into()?,
        )])))
        .await?;
    let second = updates
        .wait_until(WAIT, |state| {
            state
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| !Arc::ptr_eq(snapshot, &installed))
        })
        .await?;
    assert_eq!(
        second.snapshot.as_ref().ok_or("snapshot")?.document,
        installed.document
    );

    messages
        .send(Event::Document(update(vec![FileEntry::Unchanged(
            "never-sent".try_into()?,
        )])))
        .await?;
    updates
        .wait_until(WAIT, |state| {
            matches!(state.connection, ConnectionState::Failed(_))
        })
        .await?;
    assert!(client.state().status().is_some());
    Ok(())
}

#[tokio::test]
async fn a_document_before_initialization_fails_the_connection() -> Result<(), Box<dyn Error>> {
    let (transport, _commands, messages) = message_channel();
    let client = DiffClient::spawn(transport, ClientOptions::default());
    let mut updates = client.subscribe();
    let file = Arc::new(FileDiff::from_texts("a", "old", "new")?);
    messages
        .send(Event::Document(update(vec![FileEntry::Changed(file)])))
        .await?;

    let state = updates
        .wait_until(WAIT, |state| {
            matches!(state.connection, ConnectionState::Failed(_))
        })
        .await?;
    assert!(matches!(
        state.connection,
        ConnectionState::Failed(ClientError::Protocol(_))
    ));
    Ok(())
}

#[tokio::test]
async fn an_unsupported_protocol_version_fails_the_connection() -> Result<(), Box<dyn Error>> {
    let (transport, _commands, messages) = message_channel();
    let client = DiffClient::spawn(transport, ClientOptions::default());
    let mut updates = client.subscribe();
    messages
        .send(Event::Initialize {
            protocol_version: LIVE_PROTOCOL_VERSION + 1,
            repository_root: "/remote".to_owned(),
        })
        .await?;

    let state = updates
        .wait_until(WAIT, |state| {
            matches!(state.connection, ConnectionState::Failed(_))
        })
        .await?;
    assert!(matches!(
        state.connection,
        ConnectionState::Failed(ClientError::Remote(_))
    ));
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

struct Channel<Out, In> {
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

fn message_channel() -> (
    Channel<ClientCommand, ServerMessage>,
    Receiver<ClientCommand>,
    Sender<ServerMessage>,
) {
    let (commands_tx, commands_rx) = bounded(4);
    let (messages_tx, messages_rx) = bounded(4);
    let transport = Channel {
        tx: commands_tx,
        rx: messages_rx,
    };
    (transport, commands_rx, messages_tx)
}

fn update(files: Vec<FileEntry>) -> DocumentUpdate {
    DocumentUpdate {
        scope: DiffScope::Both,
        repo_root: "/remote".to_owned(),
        files,
    }
}
