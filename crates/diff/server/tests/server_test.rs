mod support;

#[cfg(feature = "websocket")]
use clankerdiff_client::ConnectionState;
use clankerdiff_client::{
    ClientError, ClientMessageTransport, ClientOptions, ClientSubscription, DiffClient, DiffReviewEvent, DiffScope,
    RemoteErrorCode, RepositoryAction, ReviewTarget,
    protocol::{
        client::ClientCommand,
        shared::{Event, LIVE_PROTOCOL_VERSION},
    },
};
use clankerdiff_core::Review;
use clankerdiff_git::testing::RepoFixtureBuilder;
use clankerdiff_server::ReviewCompletion;
#[cfg(feature = "websocket")]
use std::sync::Arc;
use std::{
    error::Error,
    time::{Duration, Instant},
};
use support::{TestServer, WAIT, message_transports, wait_for_text};
use tokio::time::timeout;

#[tokio::test]
async fn local_clients_have_independent_scopes_and_observe_actions() -> Result<(), Box<dyn Error>> {
    let fixture = TestServer::start().await?;
    fixture.repo.write("a", "new\n");
    let a = fixture.client().await?;
    let b = fixture.client().await?;
    let mut state_a = a.subscribe();
    let mut state_b = b.subscribe();
    wait_for(&mut state_a, DiffScope::Both, 1).await?;
    wait_for(&mut state_b, DiffScope::Both, 1).await?;
    timeout(WAIT, a.set_target(DiffScope::Staged)).await??;
    assert_eq!(a.state().snapshot.as_ref().ok_or("snapshot")?.target, ReviewTarget::WorkingTree(DiffScope::Staged));
    assert_eq!(b.state().snapshot.as_ref().ok_or("snapshot")?.target, ReviewTarget::WorkingTree(DiffScope::Both));
    a.apply(RepositoryAction::StageAll).await?;
    wait_for(&mut state_a, DiffScope::Staged, 1).await?;
    a.close().await?;
    b.refresh().await?;
    b.close().await?;
    fixture.server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn submitted_review_reaches_the_server_with_the_effective_scope() -> Result<(), Box<dyn Error>> {
    let mut fixture = TestServer::start().await?;
    let client = fixture.client().await?;
    client.set_target(DiffScope::Staged).await?;
    let submission = Review::default().submission();
    let submitting = client.clone();
    let expected = submission.clone();
    let request = tokio::spawn(async move { submitting.handle(DiffReviewEvent::SubmitReview(submission)).await });
    let completion = timeout(WAIT, fixture.server.next_review()).await??;
    assert_eq!(completion, ReviewCompletion::Submitted { target: DiffScope::Staged.into(), submission: expected });
    timeout(WAIT, request).await???;
    fixture.server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn commit_reviews_start_on_the_first_commit_and_page_to_the_working_tree() -> Result<(), Box<dyn Error>> {
    let repo = RepoFixtureBuilder::new().file("a", "old\n").committed().build();
    repo.git(&["checkout", "-b", "feature"]);
    repo.write("a", "committed\n");
    let first = repo.commit("First");
    repo.write("a", "uncommitted\n");
    let fixture = TestServer::open(repo).await?;
    assert_eq!(fixture.server.initial_target(DiffScope::Both, false), ReviewTarget::WorkingTree(DiffScope::Both));
    let initial = fixture.server.initial_target(DiffScope::Both, true);
    assert_eq!(initial, ReviewTarget::Commit(first.clone()));

    let client = DiffClient::from_transport(fixture.server.connect()?, initial.into()).await?;
    let state = client.subscribe().wait_until(WAIT, |state| state.snapshot.is_some()).await?;
    let snapshot = state.snapshot.as_ref().ok_or("snapshot")?;
    assert_eq!(snapshot.target, ReviewTarget::Commit(first.clone()));
    let commits = snapshot.commits.as_ref().ok_or("commit stack")?;
    assert_eq!(commits.commits.len(), 1);
    assert!(state.capabilities.scope && !state.capabilities.repository);

    timeout(WAIT, client.set_target(DiffScope::Both)).await??;
    let state = client.state();
    let snapshot = state.snapshot.as_ref().ok_or("snapshot")?;
    assert_eq!(snapshot.target, ReviewTarget::WorkingTree(DiffScope::Both));
    assert!(snapshot.commits.is_some());
    assert!(state.capabilities.repository);
    client.close().await?;
    fixture.server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn cancelled_review_reaches_the_server() -> Result<(), Box<dyn Error>> {
    let mut fixture = TestServer::start().await?;
    let client = fixture.client().await?;
    let cancelling = client.clone();
    let request = tokio::spawn(async move { cancelling.handle(DiffReviewEvent::Cancel).await });
    assert_eq!(
        timeout(WAIT, fixture.server.next_review()).await??,
        ReviewCompletion::Cancelled { target: DiffScope::Both.into() }
    );
    timeout(WAIT, request).await???;
    fixture.server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn only_one_concurrent_review_completion_is_accepted() -> Result<(), Box<dyn Error>> {
    let mut fixture = TestServer::start().await?;
    let first = fixture.client().await?;
    let second = fixture.client().await?;
    let first_request = tokio::spawn(async move { first.handle(DiffReviewEvent::Cancel).await });
    let second_request = tokio::spawn(async move { second.handle(DiffReviewEvent::Cancel).await });
    assert!(matches!(timeout(WAIT, fixture.server.next_review()).await??, ReviewCompletion::Cancelled { .. }));
    let first_result = timeout(WAIT, first_request).await??;
    let second_result = timeout(WAIT, second_request).await??;
    assert!(matches!(
        (&first_result, &second_result),
        (Ok(()), Err(ClientError::Remote(error)))
            | (Err(ClientError::Remote(error)), Ok(()))
            if error.code == RemoteErrorCode::Busy
    ));
    fixture.server.shutdown().await?;
    Ok(())
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn websocket_client_bootstraps_and_tracks_real_edits() -> Result<(), Box<dyn Error>> {
    let fixture = TestServer::start().await?;
    let (listener, client, mut state) = fixture.connect_ws().await?;
    fixture.repo.write("a", "new\n");
    wait_for(&mut state, DiffScope::Both, 1).await?;
    assert_eq!(
        state.latest().snapshot.as_ref().ok_or("snapshot")?.files[0].new_source.as_ref().map_err(Clone::clone)?.text(),
        "new\n"
    );
    client.close().await?;
    listener.shutdown().await?;
    fixture.server.shutdown().await?;
    Ok(())
}

#[cfg(feature = "websocket")]
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
    assert!(Arc::ptr_eq(&previous, client.state().snapshot.as_ref().ok_or("retained snapshot")?));
    fixture.repo.write("a", "after disconnect\n");
    let listener = fixture.server.listen(address).await?;
    wait_for(&mut state, DiffScope::Both, 1).await?;
    client.close().await?;
    listener.shutdown().await?;
    fixture.server.shutdown().await?;
    Ok(())
}

#[cfg(feature = "websocket")]
#[tokio::test]
async fn server_shutdown_closes_active_websocket_connections() -> Result<(), Box<dyn Error>> {
    let fixture = TestServer::start().await?;
    let (listener, client, mut state) = fixture.connect_ws().await?;

    timeout(Duration::from_secs(2), fixture.server.shutdown()).await??;
    state.wait_until(Duration::from_secs(2), |state| !matches!(state.connection, ConnectionState::Connected)).await?;
    client.close().await?;
    listener.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn continuous_writes_publish_before_the_writer_stops() -> Result<(), Box<dyn Error>> {
    let fixture = TestServer::start().await?;
    let client = fixture.client().await?;
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
        if state.latest().snapshot.as_ref().is_some_and(|snapshot| !snapshot.files.is_empty()) {
            break true;
        }
    };
    assert!(observed, "fixed-window debounce must publish while writes continue");

    revision += 1;
    let settled = format!("revision {revision}\n");
    fixture.repo.write("a", settled.clone());
    wait_for_text(&mut state, &settled).await?;
    client.close().await?;
    fixture.server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn message_transports_track_edits_and_actions_through_the_wire_form() -> Result<(), Box<dyn Error>> {
    let fixture = TestServer::start().await?;
    let (client_end, server_end) = message_transports();
    fixture.server.accept(server_end)?;
    let client = DiffClient::from_messages(client_end, ClientOptions::default()).await?;
    let mut state = client.subscribe();
    wait_for(&mut state, DiffScope::Both, 0).await?;
    fixture.repo.write("a", "new\n");
    wait_for_text(&mut state, "new\n").await?;
    client.apply(RepositoryAction::StageAll).await?;
    timeout(WAIT, client.set_target(DiffScope::Staged)).await??;
    wait_for(&mut state, DiffScope::Staged, 1).await?;
    client.close().await?;
    fixture.server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn repository_requests_sent_back_to_back_run_in_order() -> Result<(), Box<dyn Error>> {
    let fixture = TestServer::start().await?;
    let (mut client_end, server_end) = message_transports();
    fixture.server.accept(server_end)?;
    let target = DiffScope::Both.into();
    client_end.send(ClientCommand::Initialize { protocol_version: LIVE_PROTOCOL_VERSION, target }).await?;
    client_end.send(ClientCommand::Refresh).await?;
    client_end.send(ClientCommand::SetTarget(DiffScope::Staged.into())).await?;
    let mut results = Vec::new();
    let mut staged = false;
    timeout(WAIT, async {
        while results.len() < 2 || !staged {
            match client_end.recv().await? {
                Event::RequestResult(result) => results.push(result),
                Event::Document(update) => staged |= update.target == ReviewTarget::WorkingTree(DiffScope::Staged),
                Event::Error(error) => return Err(error.into()),
                _ => {}
            }
        }
        Ok::<_, Box<dyn Error>>(())
    })
    .await??;
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    fixture.server.shutdown().await?;
    Ok(())
}

async fn wait_for(state: &mut ClientSubscription, scope: DiffScope, files: usize) -> Result<(), Box<dyn Error>> {
    state
        .wait_until(WAIT, |state| {
            state.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.target == ReviewTarget::WorkingTree(scope) && snapshot.files.len() == files
            })
        })
        .await?;
    Ok(())
}
