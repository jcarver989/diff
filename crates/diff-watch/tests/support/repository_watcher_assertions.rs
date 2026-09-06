use super::wait::{assert_pending, wait_for};
use diff_watch::{Published, WatchStateReceiver};
use std::sync::Arc;

pub async fn wait_for_revision(state: &mut WatchStateReceiver, revision: u64) -> Arc<Published> {
    wait_for(&format!("revision newer than {revision}"), async {
        loop {
            let latest = state.borrow_and_update().latest.clone();
            if latest.revision > revision {
                return latest;
            }
            state.changed().await.expect("the watcher must stay alive");
        }
    })
    .await
}

pub async fn assert_revision_unchanged(state: &mut WatchStateReceiver, revision: u64) {
    assert_pending(&format!("revision must remain {revision}"), async {
        loop {
            let actual = state.borrow_and_update().latest.revision;
            if actual != revision {
                return;
            }
            state.changed().await.expect("the watcher must stay alive");
        }
    })
    .await;
}
