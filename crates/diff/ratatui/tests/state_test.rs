use clankerdiff_client::protocol::client::capabilities;
use clankerdiff_client::{ClientError, ClientState, ConnectionState};
use clankerdiff_core::{
    DiffDocument, DiffReviewEvent, DiffScope, ReviewTarget,
    testing::{DocumentBuilder, commit_id},
};
use clankerdiff_ratatui::{
    DiffReviewCommand, DiffReviewState, DiffReviewStatus, InputOutcome, InteractionPhase, KeyCode, KeyEvent,
    KeyModifiers, ReviewCapabilities, ReviewInput,
};
use std::sync::Arc;

#[test]
fn installs_a_changed_snapshot_and_skips_unchanged_pointers() {
    let mut state = DiffReviewState::loading();
    let mut installed = None;
    let snapshot = document(DiffScope::Staged, "first");

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

    assert_eq!(state.status(), &DiffReviewStatus::Error("connection closed".to_owned()));
}

#[test]
fn a_failure_after_a_document_is_a_background_error() {
    let mut state = DiffReviewState::loading();
    let mut installed = None;
    let snapshot = document(DiffScope::Both, "first");

    state.apply_client_state(&connected(Arc::clone(&snapshot)), &mut installed);
    state.apply_client_state(&failed(Some(snapshot)), &mut installed);

    assert_eq!(state.status(), &DiffReviewStatus::Ready, "the loaded document is retained");
    assert_eq!(state.repository_error(), Some("connection closed"));
}

#[test]
fn capabilities_track_the_client_state() {
    let mut state = DiffReviewState::loading();
    let mut installed = None;
    let snapshot = document(DiffScope::Both, "first");
    let mut client = connected(Arc::clone(&snapshot));
    client.capabilities = capabilities(true, Some(&snapshot));
    state.apply_client_state(&client, &mut installed);
    assert!(state.command_context().capabilities.repository);

    client.capabilities = capabilities(false, None);
    state.apply_client_state(&client, &mut installed);
    assert!(!state.command_context().capabilities.repository);
}

#[test]
fn brackets_step_through_commits_and_back_to_the_working_tree() {
    let mut state = DiffReviewState::loading();
    let mut installed = None;
    let mut install = |state: &mut DiffReviewState, target: ReviewTarget| {
        state.apply_client_state(&connected(document(target, "content")), &mut installed);
    };
    install(&mut state, DiffScope::Staged.into());
    install(&mut state, commit_id(1).into());
    assert_eq!(emitted_target(&mut state, '['), None);
    assert_eq!(emitted_target(&mut state, ']'), Some(ReviewTarget::Commit(commit_id(2))));
    install(&mut state, commit_id(2).into());
    assert_eq!(emitted_target(&mut state, ']'), Some(ReviewTarget::WorkingTree(DiffScope::Staged)));
}

#[test]
fn the_first_commit_and_the_working_tree_are_commands_without_keys() {
    let mut state = DiffReviewState::new(document(commit_id(2), "first"));
    assert_eq!(emitted_target(&mut state, '{'), None);
    assert_eq!(command_target(&mut state, DiffReviewCommand::GoToPage(0)), Some(ReviewTarget::Commit(commit_id(1))));
    assert_eq!(
        command_target(&mut state, DiffReviewCommand::GoToPage(usize::MAX)),
        Some(ReviewTarget::WorkingTree(DiffScope::Both))
    );
}

#[test]
fn the_commit_picker_opens_on_the_current_page_and_chooses_with_the_keyboard() {
    let mut state = DiffReviewState::new(document(commit_id(2), "first"));
    assert_eq!(emitted_target(&mut state, 'p'), None);
    assert_eq!(state.interaction_phase(), InteractionPhase::CommitPicker);
    assert_eq!(emitted_target(&mut state, 'k'), None);
    assert_eq!(emitted_target(&mut state, ']'), None);
    assert_eq!(state.interaction_phase(), InteractionPhase::CommitPicker);
    assert_eq!(key_target(&mut state, KeyCode::Enter), Some(ReviewTarget::Commit(commit_id(1))));
    assert_eq!(state.interaction_phase(), InteractionPhase::Browse);

    emitted_target(&mut state, 'p');
    assert_eq!(emitted_target(&mut state, '3'), Some(ReviewTarget::WorkingTree(DiffScope::Both)));
    assert_eq!(state.interaction_phase(), InteractionPhase::Browse);

    emitted_target(&mut state, 'p');
    assert_eq!(key_target(&mut state, KeyCode::Esc), None);
    assert_eq!(state.interaction_phase(), InteractionPhase::Browse);
}

#[test]
fn reviews_without_commits_have_no_picker() {
    let mut state = DiffReviewState::new(DocumentBuilder::new().changed("a.rs", "old", "new").build());
    assert!(!state.handle_input(ReviewInput::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE))).is_consumed());
    assert_eq!(state.interaction_phase(), InteractionPhase::Browse);
}

fn connected(snapshot: Arc<DiffDocument>) -> ClientState {
    ClientState {
        snapshot: Some(snapshot),
        connection: ConnectionState::Connected,
        capabilities: ReviewCapabilities::default(),
        error: None,
    }
}

fn failed(snapshot: Option<Arc<DiffDocument>>) -> ClientState {
    ClientState {
        snapshot,
        connection: ConnectionState::Failed(ClientError::Disconnected),
        capabilities: ReviewCapabilities::default(),
        error: None,
    }
}

fn document(target: impl Into<ReviewTarget>, version: &str) -> Arc<DiffDocument> {
    DocumentBuilder::new()
        .commit("First")
        .commit("Second")
        .target(target)
        .changed("file.rs", "old\n", &format!("{version}\n"))
        .build()
}

fn emitted_target(state: &mut DiffReviewState, key: char) -> Option<ReviewTarget> {
    match state.handle_input(ReviewInput::Key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE))) {
        InputOutcome::Emitted(DiffReviewEvent::SetTarget(target)) => Some(target),
        _ => None,
    }
}

fn command_target(state: &mut DiffReviewState, command: DiffReviewCommand) -> Option<ReviewTarget> {
    match state.handle_command(command) {
        InputOutcome::Emitted(DiffReviewEvent::SetTarget(target)) => Some(target),
        _ => None,
    }
}

fn key_target(state: &mut DiffReviewState, key: KeyCode) -> Option<ReviewTarget> {
    match state.handle_input(ReviewInput::Key(KeyEvent::new(key, KeyModifiers::NONE))) {
        InputOutcome::Emitted(DiffReviewEvent::SetTarget(target)) => Some(target),
        _ => None,
    }
}
