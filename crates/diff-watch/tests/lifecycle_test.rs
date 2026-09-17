use clankerdiff_core::DiffScope;
use clankerdiff_git::testing::RepoFixtureBuilder;
use clankerdiff_watch::{RepositoryRequest, RepositoryWatcher, WatchOptions};
use std::{error::Error, time::Duration};
use tokio::{sync::oneshot, time::timeout};

#[tokio::test]
async fn explicit_refresh_and_joined_shutdown() -> Result<(), Box<dyn Error>> {
    let repo = RepoFixtureBuilder::new()
        .file("a", "old\n")
        .committed()
        .build();

    let watcher = RepositoryWatcher::spawn(
        repo.repository().await,
        DiffScope::Both,
        WatchOptions {
            debounce: Duration::from_secs(60),
        },
    )
    .await?;

    let mut state = watcher.state_rx.clone();
    repo.write("a", "new\n");
    let (result_tx, result_rx) = oneshot::channel();
    watcher
        .request_tx
        .send(RepositoryRequest::Refresh { result_tx })
        .await?;

    result_rx.await??;
    assert_eq!(state.borrow_and_update().snapshot.document.files.len(), 1);

    let commands = watcher.request_tx.clone();
    timeout(Duration::from_secs(5), watcher.shutdown()).await??;
    assert!(commands.is_closed());
    assert!(state.changed().await.is_err());
    Ok(())
}
