use clankerdiff_client::{
    ClientState, ConnectionState, RemoteError, RemoteErrorCode,
    protocol::{server::ServerEvent, shared::LIVE_PROTOCOL_VERSION},
};
use clankerdiff_core::{
    DiffScope, ReviewTarget,
    testing::{DocumentBuilder, commit_id},
};

#[test]
fn reduces_initialization_then_documents() {
    let state = initialized().apply(&document(DiffScope::Staged));

    assert!(matches!(state.connection, ConnectionState::Connected));
    assert!(state.capabilities.repository && state.capabilities.submit);
    assert_eq!(state.snapshot.as_ref().expect("snapshot").target, ReviewTarget::WorkingTree(DiffScope::Staged));
}

#[test]
fn commit_pages_allow_navigation_and_submission_but_not_repository_actions() {
    let on_commit = initialized().apply(&document(commit_id(1)));
    assert!(on_commit.capabilities.scope && on_commit.capabilities.submit);
    assert!(!on_commit.capabilities.repository);
    assert!(on_commit.apply(&document(DiffScope::Both)).capabilities.repository);
}

#[test]
fn request_results_leave_state_untouched() {
    let state = initialized();
    assert_eq!(state.clone().apply(&ServerEvent::RequestResult(Ok(()))), state);
}

#[test]
fn health_replaces_only_the_error() {
    let error = RemoteError::new(RemoteErrorCode::Git, "git failed");
    let state = initialized().apply(&ServerEvent::Health { error: Some(error.clone()) });

    assert_eq!(state.error.as_ref(), Some(&error));
    assert!(matches!(state.connection, ConnectionState::Connected));
    assert!(state.capabilities.repository, "health does not affect capabilities");

    assert_eq!(state.apply(&ServerEvent::Health { error: None }).error, None);
}

#[test]
fn a_terminal_error_fails_the_connection() {
    let failed =
        initialized().apply(&ServerEvent::Error(RemoteError::new(RemoteErrorCode::Watcher, "watcher stopped")));

    assert!(matches!(failed.connection, ConnectionState::Failed(_)));
    assert!(!failed.capabilities.repository);
}

fn initialize() -> ServerEvent {
    ServerEvent::Initialize { protocol_version: LIVE_PROTOCOL_VERSION, repository_root: "/repo".to_owned() }
}

fn document(target: impl Into<ReviewTarget>) -> ServerEvent {
    ServerEvent::Document(DocumentBuilder::new().target(target).changed("a", "old", "new").build())
}

fn initialized() -> ClientState {
    ClientState::default().apply(&initialize()).apply(&document(DiffScope::Both))
}
