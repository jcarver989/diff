use super::{
    test_result::TestResult,
    wait::{assert_pending, wait_for},
};
use clankerdiff_git::RepositorySnapshot;
use clankerdiff_watch::RepositoryState;
use std::sync::Arc;
use tokio::sync::watch;

/// The retained snapshot, or the load error the watcher is currently reporting.
pub fn healthy(state: &RepositoryState) -> TestResult<Arc<RepositorySnapshot>> {
    match &state.error {
        Some(error) => Err(Arc::clone(error).into()),
        None => Ok(Arc::clone(&state.snapshot)),
    }
}

pub async fn wait_for_snapshot(
    state: &mut watch::Receiver<RepositoryState>,
) -> TestResult<Arc<RepositorySnapshot>> {
    wait_for("snapshot publication", state.changed()).await?;
    healthy(&state.borrow_and_update())
}

pub async fn assert_snapshot_unchanged(state: &mut watch::Receiver<RepositoryState>) {
    state.borrow_and_update();
    assert_pending("snapshot must remain unchanged", state.changed()).await;
}
