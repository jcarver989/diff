use clankerdiff_client::protocol::client::capabilities;
use clankerdiff_client::{ClientError, ClientState, ConnectionState, DiffSnapshot};
use clankerdiff_core::{DiffScope, testing::DocumentBuilder};
use clankerdiff_ratatui::{DiffReviewState, DiffReviewStatus, ReviewCapabilities};
use std::sync::Arc;

#[test]
fn installs_a_changed_snapshot_and_skips_unchanged_pointers() {
    let mut state = DiffReviewState::loading();
    let mut installed = None;
    let snapshot = Arc::new(DiffSnapshot {
        scope: DiffScope::Staged,
        document: document("first"),
    });

    state.apply_client_state(&connected(Arc::clone(&snapshot)), &mut installed);
    assert_eq!(state.scope(), DiffScope::Staged);
    assert_eq!(state.status(), &DiffReviewStatus::Ready);
    let installed_document = state.document().clone();

    state.apply_client_state(&connected(snapshot), &mut installed);
    assert!(
        Arc::ptr_eq(state.document(), &installed_document),
        "an unchanged snapshot pointer must not reinstall the document"
    );
}

#[test]
fn a_failure_without_a_document_is_fatal() {
    let mut state = DiffReviewState::loading();
    let mut installed = None;

    state.apply_client_state(&failed(None), &mut installed);

    assert_eq!(
        state.status(),
        &DiffReviewStatus::Error("connection closed".to_owned())
    );
}

#[test]
fn a_failure_after_a_document_is_a_background_error() {
    let mut state = DiffReviewState::loading();
    let mut installed = None;
    let snapshot = Arc::new(DiffSnapshot {
        scope: DiffScope::Both,
        document: document("first"),
    });

    state.apply_client_state(&connected(Arc::clone(&snapshot)), &mut installed);
    state.apply_client_state(&failed(Some(snapshot)), &mut installed);

    assert_eq!(
        state.status(),
        &DiffReviewStatus::Ready,
        "the loaded document is retained"
    );
    assert_eq!(state.repository_error(), Some("connection closed"));
}

#[test]
fn capabilities_track_the_client_state() {
    let mut state = DiffReviewState::loading();
    let mut installed = None;
    let snapshot = Arc::new(DiffSnapshot {
        scope: DiffScope::Both,
        document: document("first"),
    });
    let mut client = connected(snapshot);
    client.capabilities = capabilities(true, true);
    state.apply_client_state(&client, &mut installed);
    assert!(state.command_context().capabilities.repository);

    client.capabilities = capabilities(false, false);
    state.apply_client_state(&client, &mut installed);
    assert!(!state.command_context().capabilities.repository);
}

fn connected(snapshot: Arc<DiffSnapshot>) -> ClientState {
    ClientState {
        snapshot: Some(snapshot),
        connection: ConnectionState::Connected,
        capabilities: ReviewCapabilities::default(),
        error: None,
    }
}

fn failed(snapshot: Option<Arc<DiffSnapshot>>) -> ClientState {
    ClientState {
        snapshot,
        connection: ConnectionState::Failed(ClientError::Disconnected),
        capabilities: ReviewCapabilities::default(),
        error: None,
    }
}

fn document(version: &str) -> Arc<clankerdiff_core::DiffDocument> {
    DocumentBuilder::new()
        .changed("file.rs", "old\n", &format!("{version}\n"))
        .build()
}
