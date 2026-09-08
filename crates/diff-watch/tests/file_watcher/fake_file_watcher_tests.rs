#[allow(dead_code)]
#[path = "../support/file_watcher_assertions.rs"]
mod file_watcher_assertions;
#[allow(dead_code)]
#[path = "../support/wait.rs"]
mod wait;

#[path = "../support/test_result.rs"]
mod test_result;

use diff_watch::file_watcher::FakeFileWatcher;
use file_watcher_assertions::{assert_invalidated, assert_watcher_closed};
use test_result::TestResult;
use wait::wait_for;

#[tokio::test]
async fn fake_coalesces_and_drains_before_ending() -> TestResult {
    let (mut watcher, handle) = FakeFileWatcher::new();
    for _ in 0..10 {
        handle.trigger_notification()?;
    }
    assert_invalidated(&mut watcher).await;
    handle.trigger_notification()?;
    drop(handle);
    assert_invalidated(&mut watcher).await;
    assert_watcher_closed(&mut watcher).await;
    Ok(())
}

#[tokio::test]
async fn dropping_fake_closes_handles() {
    let (watcher, handle) = FakeFileWatcher::new();
    drop(watcher);
    wait_for("fake watcher handle closure", handle.closed()).await;
    assert!(handle.trigger_notification().is_err());
}
