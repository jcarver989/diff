#[path = "support/repository_watcher_assertions.rs"]
mod repository_watcher_assertions;
#[path = "support/test_result.rs"]
mod test_result;
#[path = "support/wait.rs"]
mod wait;

use clankerdiff_core::{DiffScope, RepoPath, RepositoryAction, StageState};
use clankerdiff_git::{
    RepositorySnapshot,
    testing::{RepoFixture, RepoFixtureBuilder},
};
use clankerdiff_watch::{RepositoryRequest, RepositoryWatcher, WatchOptions};
use repository_watcher_assertions::{assert_snapshot_unchanged, wait_for_snapshot};
use std::time::Duration;
use test_result::TestResult;
use tokio::sync::oneshot;
use wait::{observe, wait_for, wait_until};

#[tokio::test]
async fn a_worktree_edit_publishes_a_snapshot_with_the_new_hunk() -> TestResult {
    let repo = RepoFixtureBuilder::new()
        .file("src/lib.rs", "fn main() {}\n")
        .committed()
        .build();
    let watcher = watcher(&repo).await?;
    let mut state = watcher.snapshot_rx.clone();
    repo.write("src/lib.rs", "fn main() {}\nfn added() {}\n");
    let published = wait_for_snapshot(&mut state).await?;
    assert_eq!(added_lines(&published, "src/lib.rs"), vec!["fn added() {}"]);
    Ok(())
}

#[tokio::test]
async fn tracked_worktree_lock_files_are_not_filtered_as_metadata() -> TestResult {
    let repo = RepoFixtureBuilder::new()
        .file("Cargo.lock", "original\n")
        .gitignore(&["*.lock"])
        .committed()
        .build();
    repo.git(&["add", "--force", "Cargo.lock"]);
    repo.git(&["commit", "-m", "track lock file"]);
    let watcher = watcher(&repo).await?;
    let mut state = watcher.snapshot_rx.clone();
    repo.write("Cargo.lock", "updated\n");
    let published = wait_for_snapshot(&mut state).await?;
    assert_eq!(added_lines(&published, "Cargo.lock"), vec!["updated"]);
    Ok(())
}

#[tokio::test]
async fn staging_is_observed_without_extra_notifications() -> TestResult {
    let repo = RepoFixtureBuilder::new()
        .file("src/lib.rs", "fn main() {}\n")
        .committed()
        .build();

    repo.write("src/lib.rs", "fn main() {}\nfn added() {}\n");
    let watcher = watcher(&repo).await?;
    repo.repository()
        .await
        .apply(RepositoryAction::StagePaths(vec![RepoPath::new(
            "src/lib.rs",
        )?]))
        .await?;

    let published = wait_for_snapshot(&mut watcher.snapshot_rx.clone()).await?;
    assert_eq!(
        stage_state(&published, "src/lib.rs"),
        Some(StageState::Staged)
    );
    assert_snapshot_unchanged(&mut watcher.snapshot_rx.clone()).await;
    Ok(())
}

#[tokio::test]
async fn host_mutations_are_observed_through_filesystem_events() -> TestResult {
    let repo = RepoFixtureBuilder::new()
        .file("file.txt", "original\n")
        .committed()
        .build();
    repo.write("file.txt", "updated\n");

    let repository = repo.repository().await;
    let watcher = watcher(&repo).await?;
    let mut snapshots = watcher.snapshot_rx.clone();
    repository.apply(RepositoryAction::StageAll).await?;
    let staged = wait_for_snapshot(&mut snapshots).await?;

    assert_eq!(stage_state(&staged, "file.txt"), Some(StageState::Staged));

    repository.apply(RepositoryAction::UnstageAll).await?;
    let unstaged = wait_for_snapshot(&mut snapshots).await?;

    assert_eq!(
        stage_state(&unstaged, "file.txt"),
        Some(StageState::Unstaged)
    );

    repository
        .apply(RepositoryAction::Discard {
            path: RepoPath::new("file.txt")?,
            status: clankerdiff_core::FileStatus::Modified,
        })
        .await?;

    let discarded = wait_for_snapshot(&mut snapshots).await?;
    assert!(discarded.document.files.is_empty());

    repo.write("file.txt", "committed\n");
    wait_for_snapshot(&mut snapshots).await?;
    repository.apply(RepositoryAction::StageAll).await?;
    let staged = wait_for_snapshot(&mut snapshots).await?;

    assert_eq!(stage_state(&staged, "file.txt"), Some(StageState::Staged));

    repository
        .apply(RepositoryAction::Commit {
            message: "watched commit".into(),
        })
        .await?;
    let committed = wait_for_snapshot(&mut snapshots).await?;
    assert!(committed.document.files.is_empty());
    Ok(())
}

#[tokio::test]
async fn ignored_edits_do_not_publish_or_hide_relevant_edits() -> TestResult {
    let repo = RepoFixtureBuilder::new()
        .file("src/lib.rs", "fn main() {}\n")
        .file("target/debug/existing.bin", "noise\n")
        .gitignore(&["target/"])
        .committed()
        .build();
    let watcher = watcher(&repo).await?;
    let mut state = watcher.snapshot_rx.clone();
    for index in 0..5 {
        repo.write(&format!("target/debug/artifact-{index}.bin"), "noise\n");
    }
    assert_snapshot_unchanged(&mut state).await;
    repo.write(".git/index.lock", "scratch");
    assert_snapshot_unchanged(&mut state).await;
    repo.remove(".git/index.lock");
    repo.write("target/debug/existing.bin", "more noise\n");
    repo.write("src/lib.rs", "fn main() {}\nfn added() {}\n");
    let published = wait_for_snapshot(&mut state).await?;
    assert_eq!(added_lines(&published, "src/lib.rs"), vec!["fn added() {}"]);
    Ok(())
}

#[tokio::test]
async fn changing_the_scope_republishes_for_the_new_scope() -> TestResult {
    let repo = RepoFixtureBuilder::new()
        .file("src/lib.rs", "fn main() {}\n")
        .committed()
        .build();
    repo.write("src/lib.rs", "fn main() {}\nfn staged() {}\n");
    repo.git(&["add", "src/lib.rs"]);
    repo.write(
        "src/lib.rs",
        "fn main() {}\nfn staged() {}\nfn unstaged() {}\n",
    );
    let watcher = watcher(&repo).await?;
    let (result_tx, completion) = oneshot::channel();
    watcher
        .request_tx
        .send(RepositoryRequest::SetScope {
            scope: DiffScope::Staged,
            result_tx,
        })
        .await?;
    wait_for("scope acknowledgement", completion).await??;
    let published = watcher.snapshot_rx.borrow().clone()?;
    assert_eq!(published.scope, DiffScope::Staged);
    assert!(watcher.snapshot_rx.has_changed()?);
    assert_eq!(
        added_lines(&published, "src/lib.rs"),
        vec!["fn staged() {}"]
    );
    Ok(())
}

#[tokio::test]
async fn a_scope_only_change_notifies_but_repeated_scope_does_not() -> TestResult {
    let repo = RepoFixtureBuilder::new()
        .file("file.txt", "original\n")
        .committed()
        .build();
    let watcher = watcher(&repo).await?;
    let original = watcher.snapshot_rx.borrow().clone()?;
    let (result_tx, completion) = oneshot::channel();
    watcher
        .request_tx
        .send(RepositoryRequest::SetScope {
            scope: DiffScope::Staged,
            result_tx,
        })
        .await?;
    wait_for("scope acknowledgement", completion).await??;
    let staged = watcher.snapshot_rx.borrow().clone()?;
    assert_eq!(original.scope, DiffScope::Both);
    let mut updates = watcher.snapshot_rx.clone();
    assert!(updates.has_changed()?);
    updates.borrow_and_update();
    assert_eq!(staged.scope, DiffScope::Staged);
    assert_eq!(original.document, staged.document);
    let (result_tx, completion) = oneshot::channel();
    watcher
        .request_tx
        .send(RepositoryRequest::SetScope {
            scope: DiffScope::Staged,
            result_tx,
        })
        .await?;
    wait_for("scope acknowledgement", completion).await??;
    assert!(!updates.has_changed()?);
    Ok(())
}

#[tokio::test]
async fn a_burst_of_edits_publishes_the_latest_contents_and_settles() -> TestResult {
    let repo = RepoFixtureBuilder::new()
        .file("file.txt", "original\n")
        .committed()
        .build();
    let watcher = watcher(&repo).await?;
    let mut snapshots = watcher.snapshot_rx.clone();
    for index in 0..50 {
        repo.write("file.txt", format!("version {index}\n"));
    }
    let published = wait_for_snapshot(&mut snapshots).await?;
    assert_eq!(added_lines(&published, "file.txt"), vec!["version 49"]);
    assert_snapshot_unchanged(&mut snapshots).await;
    Ok(())
}

#[tokio::test]
async fn linked_worktree_index_head_and_shared_refs_are_watched() -> TestResult {
    let main = RepoFixtureBuilder::new()
        .file("file.txt", "old\n")
        .committed()
        .build();
    let repo = main.linked_worktree("review");
    repo.write("file.txt", "new\n");
    let watcher = watcher(&repo).await?;
    let mut state = watcher.snapshot_rx.clone();
    assert_eq!(
        stage_state(state.borrow().clone()?.as_ref(), "file.txt"),
        Some(StageState::Unstaged)
    );
    repo.git(&["add", "file.txt"]);
    let staged = wait_for_snapshot(&mut state).await?;
    assert_eq!(stage_state(&staged, "file.txt"), Some(StageState::Staged));
    repo.git(&["commit", "-m", "linked change"]);
    let committed = wait_for_snapshot(&mut state).await?;
    assert!(committed.document.files.is_empty());
    main.git(&["branch", "saved-review", "review"]);
    main.git(&["update-ref", "refs/heads/review", "main"]);
    let reset = wait_for_snapshot(&mut state).await?;
    assert_eq!(stage_state(&reset, "file.txt"), Some(StageState::Staged));
    repo.git(&["symbolic-ref", "HEAD", "refs/heads/saved-review"]);
    let switched = wait_for_snapshot(&mut state).await?;
    assert!(switched.document.files.is_empty());
    Ok(())
}

#[tokio::test]
async fn background_recovery_publishes_success_even_with_unchanged_content() -> TestResult {
    let repo = RepoFixtureBuilder::new()
        .file("file.txt", "old\n")
        .committed()
        .build();
    let watcher = watcher(&repo).await?;
    let mut state = watcher.snapshot_rx.clone();
    let original = state.borrow().clone()?;
    let index = std::fs::read(repo.root().join(".git/index"))?;
    repo.write(".git/index", "corrupt index");
    wait_for("background failure publication", async {
        loop {
            if state.borrow_and_update().is_err() {
                break;
            }
            state.changed().await?;
        }
        TestResult::Ok(())
    })
    .await?;
    let original_error = state.borrow().as_ref().err().map(ToString::to_string);
    assert!(watcher.snapshot_rx.clone().borrow().is_err());
    let (result_tx, completion) = oneshot::channel();
    watcher
        .request_tx
        .send(RepositoryRequest::SetScope {
            scope: DiffScope::Both,
            result_tx,
        })
        .await?;
    wait_for("repeated failure acknowledgement", completion)
        .await?
        .expect_err("the index is still corrupt");
    assert_eq!(
        state.borrow().as_ref().err().map(ToString::to_string),
        original_error
    );
    assert!(!state.has_changed()?);
    repo.write(".git/index", index);
    wait_for("equal-content recovery notification", async {
        loop {
            state.changed().await?;
            if state.borrow_and_update().is_ok() {
                break;
            }
        }
        TestResult::Ok(())
    })
    .await?;
    assert_eq!(state.borrow().clone()?, original);
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn changes_from_a_failed_mutation_are_observed() -> TestResult {
    let repo = RepoFixtureBuilder::new()
        .file("file.txt", "old\n")
        .committed()
        .build();
    repo.write("file.txt", "staged\n");
    repo.git(&["add", "file.txt"]);
    repo.write_executable(
        ".git/hooks/pre-commit",
        "#!/bin/sh\nprintf 'hook changed\\n' > file.txt\nexit 1\n",
    );
    let watcher = watcher(&repo).await?;
    let result = repo
        .repository()
        .await
        .apply(RepositoryAction::Commit {
            message: "rejected by hook".into(),
        })
        .await;
    assert!(result.is_err());
    let published = wait_for_snapshot(&mut watcher.snapshot_rx.clone()).await?;
    assert!(added_lines(&published, "file.txt").contains(&"hook changed".to_owned()));
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn cancelling_a_host_owned_mutation_leaves_the_watcher_running() -> TestResult {
    let repo = RepoFixtureBuilder::new()
        .file("file.txt", "old\n")
        .committed()
        .build();
    repo.write("file.txt", "new\n");
    repo.git(&["add", "file.txt"]);
    repo.write_executable(".git/hooks/pre-commit", "#!/bin/sh\n: > .git/hook-started\ni=0\nwhile [ ! -f .git/hook-release ] && [ $i -lt 500 ]; do\n  sleep 0.01\n  i=$((i + 1))\ndone\n");
    let watcher = watcher(&repo).await?;
    let repository = repo.repository().await;
    let command = tokio::spawn(async move {
        repository
            .apply(RepositoryAction::Commit {
                message: "must be cancelled".into(),
            })
            .await
    });
    wait_until("Git to enter the hook", || {
        repo.root().join(".git/hook-started").exists()
    })
    .await;
    command.abort();
    assert!(
        command
            .await
            .expect_err("the host task was cancelled")
            .is_cancelled()
    );
    repo.write("file.txt", "after cancellation\n");
    let published = wait_for_snapshot(&mut watcher.snapshot_rx.clone()).await?;
    assert!(added_lines(&published, "file.txt").contains(&"after cancellation".to_owned()));
    repo.write(".git/hook-release", "");
    observe().await;
    let snapshot = repo.repository().await.snapshot(DiffScope::Staged).await?;
    assert!(
        !snapshot.files.is_empty(),
        "the cancelled commit must not finish after its owner is dropped"
    );
    Ok(())
}

#[tokio::test]
async fn watched_snapshots_are_immutable_and_scope_requests_publish_before_acknowledging()
-> TestResult {
    let repo = RepoFixtureBuilder::new()
        .file("file.txt", "old\n")
        .committed()
        .build();
    let repository = repo.repository().await;
    let watcher = watcher(&repo).await?;
    let mut snapshots = watcher.snapshot_rx.clone();
    let original = snapshots.borrow_and_update().clone()?;
    for contents in ["first\n", "second\n", "third\n"] {
        repo.write("file.txt", contents);
        wait_for_snapshot(&mut snapshots).await?;
        assert_eq!(
            added_lines(snapshots.borrow().clone()?.as_ref(), "file.txt"),
            vec![contents.trim_end()]
        );
    }
    assert!(original.document.files.is_empty());
    assert!(!snapshots.has_changed()?);
    let expected = repository.snapshot_with_sources(DiffScope::Both).await?;
    assert_eq!(*snapshots.borrow().clone()?, expected);
    let (result_tx, completion) = oneshot::channel();
    watcher
        .request_tx
        .send(RepositoryRequest::SetScope {
            scope: DiffScope::Staged,
            result_tx,
        })
        .await?;
    wait_for("raw scope acknowledgement", completion).await??;
    assert_eq!(snapshots.borrow().clone()?.scope, DiffScope::Staged);
    assert!(snapshots.borrow().clone()?.document.files.is_empty());
    Ok(())
}

#[tokio::test]
async fn dropping_the_watcher_closes_queued_replies_and_channels() -> TestResult {
    let repo = RepoFixtureBuilder::new()
        .file("file.txt", "old\n")
        .committed()
        .build();
    let watcher = watcher(&repo).await?;
    let requests = watcher.request_tx.clone();
    let mut snapshots = watcher.snapshot_rx.clone();
    let (result_tx, completion) = oneshot::channel();
    requests
        .send(RepositoryRequest::SetScope {
            scope: DiffScope::Staged,
            result_tx,
        })
        .await?;
    drop(watcher);
    wait_for("queued reply closure", completion)
        .await
        .expect_err("the stopped actor cannot acknowledge a scope change");
    wait_for("snapshot channel closure", snapshots.changed())
        .await
        .expect_err("the snapshot publisher must stop");
    let (result_tx, _completion) = oneshot::channel();
    requests
        .send(RepositoryRequest::SetScope {
            scope: DiffScope::Staged,
            result_tx,
        })
        .await
        .expect_err("a stopped watcher rejects requests");
    Ok(())
}

fn options() -> WatchOptions {
    WatchOptions {
        debounce: Duration::from_millis(50),
    }
}

async fn watcher(repo: &RepoFixture) -> TestResult<RepositoryWatcher> {
    Ok(RepositoryWatcher::spawn(repo.repository().await, DiffScope::Both, options()).await?)
}

fn added_lines(published: &RepositorySnapshot, path: &str) -> Vec<String> {
    published
        .document
        .files
        .iter()
        .filter(|file| file.path.as_str() == path)
        .flat_map(|file| file.hunks.iter())
        .flat_map(|hunk| hunk.lines.iter())
        .filter(|line| line.new_line_no.is_some() && line.old_line_no.is_none())
        .map(|line| line.text.to_string())
        .collect()
}

fn stage_state(published: &RepositorySnapshot, path: &str) -> Option<StageState> {
    published
        .document
        .files
        .iter()
        .find(|file| file.path.as_str() == path)
        .map(|file| file.staged)
}
