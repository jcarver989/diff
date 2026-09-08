use super::{
    test_result::TestResult,
    wait::{assert_pending, wait_for},
};
use clankerdiff_git::{GitError, RepositorySnapshot};
use std::sync::Arc;
use tokio::sync::watch;

pub async fn wait_for_snapshot(
    state: &mut watch::Receiver<Result<Arc<RepositorySnapshot>, Arc<GitError>>>,
) -> TestResult<Arc<RepositorySnapshot>> {
    wait_for("snapshot publication", state.changed()).await?;
    Ok(state.borrow_and_update().clone()?)
}

pub async fn assert_snapshot_unchanged(
    state: &mut watch::Receiver<Result<Arc<RepositorySnapshot>, Arc<GitError>>>,
) {
    state.borrow_and_update();
    assert_pending("snapshot must remain unchanged", state.changed()).await;
}
