use super::wait::{assert_pending, wait_for};
use clankerdiff_watch::file_watcher::FileWatcher;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

pub async fn assert_invalidated(watcher: &mut impl FileWatcher) {
    assert_eq!(wait_for("invalidation", watcher.recv()).await, Some(()));
}

pub async fn assert_no_invalidation(watcher: &mut impl FileWatcher) {
    assert_pending("watcher must not invalidate or close", watcher.recv()).await;
}

pub async fn assert_watcher_closed(watcher: &mut impl FileWatcher) {
    assert_eq!(wait_for("watcher closure", watcher.recv()).await, None);
}

pub async fn wait_for_path(batches: &mut mpsc::UnboundedReceiver<Vec<PathBuf>>, path: &Path) {
    wait_for(
        &format!("filter batch containing {}", path.display()),
        async {
            loop {
                let paths = batches.recv().await.expect("filter remains alive");
                let mut unique = paths.clone();
                unique.sort();
                unique.dedup();
                assert_eq!(
                    paths.len(),
                    unique.len(),
                    "filter batches contain no duplicate paths"
                );
                if paths.iter().any(|candidate| candidate == path) {
                    break;
                }
            }
        },
    )
    .await;
}
