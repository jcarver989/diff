#[path = "../support/temp_dir_builder.rs"]
mod temp_dir_builder;

#[allow(dead_code)]
#[path = "../support/file_watcher_assertions.rs"]
mod file_watcher_assertions;
#[allow(dead_code)]
#[path = "../support/wait.rs"]
mod wait;

#[path = "../support/test_result.rs"]
mod test_result;

use diff_watch::file_watcher::NotifyFileWatcher;
use file_watcher_assertions::{assert_invalidated, assert_no_invalidation, wait_for_path};
use std::{fs, future::pending, time::Duration};
use temp_dir_builder::TempDirBuilder;
use test_result::TestResult;
use tokio::sync::{mpsc, oneshot};
use wait::wait_for;

#[tokio::test]
async fn filters_real_edits_and_observes_nested_overlapping_roots() -> TestResult {
    let (observed, mut batches) = mpsc::unbounded_channel();
    let directory = TempDirBuilder::new().entries([("nested", "")]).build()?;
    let root = directory.path().canonicalize()?;
    let mut watcher = NotifyFileWatcher::new(
        [root.clone(), root.join("nested"), root.clone()],
        Duration::from_millis(50),
        move |paths| {
            let is_watched = paths
                .iter()
                .any(|path| path.extension().is_some_and(|ext| ext == "txt"));
            assert!(
                observed.send(paths).is_ok(),
                "batch receiver must remain open"
            );
            async move { is_watched }
        },
    )?;

    fs::write(root.join("nested/ignored.bin"), "ignored")?;
    wait_for_path(&mut batches, &root.join("nested/ignored.bin")).await;
    assert_no_invalidation(&mut watcher).await;
    for index in 0..20 {
        fs::write(root.join("nested/wanted.txt"), format!("edit {index}"))?;
    }
    wait_for_path(&mut batches, &root.join("nested/wanted.txt")).await;
    assert_invalidated(&mut watcher).await;
    Ok(())
}

#[tokio::test]
async fn reading_files_does_not_invalidate() -> TestResult {
    let directory = TempDirBuilder::new()
        .entries([("existing.txt", "content")])
        .build()?;

    let root = directory.path().canonicalize()?;
    let mut watcher =
        NotifyFileWatcher::new([root.clone()], Duration::from_millis(50), |_| async {
            true
        })?;

    for _ in 0..20 {
        assert_eq!(fs::read_to_string(root.join("existing.txt"))?, "content");
    }

    assert_no_invalidation(&mut watcher).await;
    Ok(())
}

#[tokio::test]
async fn dropping_watcher_cancels_the_callers_in_flight_filter() -> TestResult {
    let (entered, ready) = oneshot::channel();
    let (lifetime, cancelled) = oneshot::channel::<()>();
    let mut guards = Some((entered, lifetime));
    let directory = TempDirBuilder::new().build()?;
    let root = directory.path().canonicalize()?;
    let watcher = NotifyFileWatcher::new([root.clone()], Duration::from_millis(50), move |_| {
        let (entered, lifetime) = guards.take().expect("first filter never completes");
        async move {
            let _guard = lifetime;
            let _ = entered.send(());
            pending::<bool>().await
        }
    })?;
    fs::write(root.join("edit.txt"), "changed")?;
    wait_for("filter start", ready).await?;
    drop(watcher);
    assert!(wait_for("filter cancellation", cancelled).await.is_err());
    Ok(())
}
