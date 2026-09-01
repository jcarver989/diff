//! Contracts for debounced native repository invalidation.

use diff_git::{RepositoryWatcher, WATCH_DEBOUNCE};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

#[test]
fn watches_worktree_and_git_metadata_changes() {
    let repo = Repo::init();
    let watcher = RepositoryWatcher::new(&repo.root).expect("start watcher");

    fs::write(repo.root.join("tracked.txt"), "modified\n").expect("modify tracked file");
    wait_for_change(&watcher);

    fs::write(repo.root.join("untracked.txt"), "new\n").expect("create untracked file");
    wait_for_change(&watcher);

    let temporary = repo.root.join("atomic.tmp");
    fs::write(&temporary, "replacement\n").expect("write atomic temporary file");
    fs::rename(&temporary, repo.root.join("tracked.txt")).expect("rename over tracked file");
    wait_for_change(&watcher);

    fs::remove_file(repo.root.join("untracked.txt")).expect("delete untracked file");
    wait_for_change(&watcher);

    repo.git(&["add", "tracked.txt"]);
    wait_for_change(&watcher);
}

#[test]
fn write_bursts_keep_only_one_pending_invalidation() {
    let repo = Repo::init();
    let watcher = RepositoryWatcher::new(&repo.root).expect("start watcher");
    let receiver = watcher.receiver();

    for index in 0..50 {
        fs::write(repo.root.join("tracked.txt"), format!("change {index}\n")).expect("write burst");
    }
    thread::sleep(WATCH_DEBOUNCE + Duration::from_millis(500));

    assert_eq!(receiver.len(), 1);
    assert!(watcher.drain().expect("healthy watcher"));
    assert_eq!(receiver.len(), 0);
}

struct Repo {
    _dir: TempDir,
    root: PathBuf,
}

impl Repo {
    fn init() -> Self {
        let dir = TempDir::new().expect("temporary directory");
        let root = dir.path().canonicalize().expect("canonical temporary path");
        let repo = Self { _dir: dir, root };
        repo.git(&["init", "--initial-branch=main"]);
        repo.git(&["config", "user.name", "Diff Watch Test"]);
        repo.git(&["config", "user.email", "diff@example.com"]);
        fs::write(repo.root.join("tracked.txt"), "base\n").expect("write tracked fixture");
        repo.git(&["add", "-A"]);
        repo.git(&["commit", "-m", "fixture"]);
        repo
    }

    fn git(&self, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(&self.root)
            .args(args)
            .output()
            .expect("git must run");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn wait_for_change(watcher: &RepositoryWatcher) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match watcher.drain() {
            Ok(true) => return,
            Ok(false) => {}
            Err(error) => panic!("watcher failed: {error}"),
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for watch event"
        );
        thread::sleep(Duration::from_millis(25));
    }
}
