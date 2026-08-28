//! Contracts between `diff-git`, the real Git executable, and `diff-core`.

use diff_core::{
    DiffDocument, DiffScope, FileDiff, FileStatus, PatchLineKind, RepoPath, StageState,
};
use diff_git::{FileContent, GitError, GitRepository};
use std::{fs, path::PathBuf, process::Command};
use tempfile::TempDir;

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
        repo.git(&["config", "user.name", "Diff Contract Test"]);
        repo.git(&["config", "user.email", "diff@example.com"]);
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

    fn write(&self, path: &str, contents: impl AsRef<[u8]>) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().expect("file has parent")).expect("create parents");
        fs::write(path, contents).expect("write fixture");
    }

    fn remove(&self, path: &str) {
        fs::remove_file(self.root.join(path)).expect("remove fixture");
    }

    fn commit_all(&self) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-m", "fixture"]);
    }

    async fn repository(&self) -> GitRepository {
        GitRepository::discover(&self.root)
            .await
            .expect("discover fixture repository")
    }
}

fn file<'a>(document: &'a DiffDocument, path: &str) -> &'a FileDiff {
    document
        .files
        .iter()
        .find(|file| file.path.as_str() == path)
        .unwrap_or_else(|| panic!("{path} missing from {:?}", paths(document)))
}

fn paths(document: &DiffDocument) -> Vec<&str> {
    document
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect()
}

fn path(value: &str) -> RepoPath {
    RepoPath::new(value).expect("valid fixture path")
}

#[tokio::test]
async fn discovers_from_a_subdirectory_and_rejects_non_repositories() {
    let repo = Repo::init();
    let nested = repo.root.join("nested/deep");
    fs::create_dir_all(&nested).expect("create nested directory");

    let discovered = GitRepository::discover(&nested).await.expect("discover");
    assert_eq!(discovered.root(), repo.root);

    let outside = TempDir::new().expect("temporary directory");
    assert!(matches!(
        GitRepository::discover(outside.path()).await,
        Err(GitError::NotRepository)
    ));
}

#[tokio::test]
async fn snapshots_keep_staged_unstaged_and_partial_content_separate() {
    let repo = Repo::init();
    repo.write("partial.txt", "base\n");
    repo.write("unstaged.txt", "base\n");
    repo.write("staged.txt", "base\n");
    repo.commit_all();

    repo.write("partial.txt", "index\n");
    repo.git(&["add", "partial.txt"]);
    repo.write("partial.txt", "worktree\n");
    repo.write("unstaged.txt", "working\n");
    repo.write("staged.txt", "indexed\n");
    repo.git(&["add", "staged.txt"]);

    let repository = repo.repository().await;
    let staged = repository
        .snapshot(DiffScope::Staged)
        .await
        .expect("staged snapshot");
    assert_eq!(paths(&staged), ["partial.txt", "staged.txt"]);
    assert_eq!(
        file(&staged, "partial.txt").staged,
        StageState::PartiallyStaged
    );
    assert!(
        file(&staged, "partial.txt")
            .hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .any(|line| line.kind == PatchLineKind::Added && line.text.as_ref() == "index")
    );

    let unstaged = repository
        .snapshot(DiffScope::Unstaged)
        .await
        .expect("unstaged snapshot");
    assert_eq!(paths(&unstaged), ["partial.txt", "unstaged.txt"]);
    assert!(
        file(&unstaged, "partial.txt")
            .hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .any(|line| line.kind == PatchLineKind::Added && line.text.as_ref() == "worktree")
    );

    let both = repository
        .snapshot(DiffScope::Both)
        .await
        .expect("combined snapshot");
    assert_eq!(paths(&both), ["partial.txt", "staged.txt", "unstaged.txt"]);
    assert_eq!(
        file(&both, "partial.txt").staged,
        StageState::PartiallyStaged
    );
}

#[tokio::test]
async fn unborn_repository_snapshots_and_unstaging_work() {
    let repo = Repo::init();
    repo.write("staged.txt", "index\n");
    repo.git(&["add", "staged.txt"]);
    repo.write("staged.txt", "worktree\n");
    repo.write("untracked.txt", "new\n");
    let repository = repo.repository().await;

    let combined = repository
        .snapshot(DiffScope::Both)
        .await
        .expect("combined unborn snapshot");
    assert_eq!(
        file(&combined, "staged.txt").staged,
        StageState::PartiallyStaged
    );
    assert_eq!(
        file(&combined, "untracked.txt").status,
        FileStatus::Untracked
    );

    repository
        .unstage_all()
        .await
        .expect("unstage unborn index");
    let staged = repository
        .snapshot(DiffScope::Staged)
        .await
        .expect("empty staged snapshot");
    assert!(staged.files.is_empty());
    assert!(
        repo.root.join("staged.txt").exists(),
        "unstaging must preserve worktree files"
    );
}

#[tokio::test]
async fn untracked_text_binary_and_utf8_paths_are_loaded_without_loss() {
    let repo = Repo::init();
    repo.write("space and é.txt", "hello\n");
    repo.write("tab\tand\nnewline.txt", "odd path\n");
    repo.write("data.bin", [0, 159, 146, 150]);
    let repository = repo.repository().await;

    let document = repository
        .snapshot(DiffScope::Unstaged)
        .await
        .expect("snapshot");
    assert!(!file(&document, "space and é.txt").binary);
    assert!(!file(&document, "tab\tand\nnewline.txt").binary);
    assert!(file(&document, "data.bin").binary);
    assert_eq!(
        repository
            .read_worktree_file(&path("space and é.txt"))
            .await
            .expect("read text"),
        FileContent::Text("hello\n".to_owned())
    );
    assert!(
        repository
            .read_worktree_file(&path("data.bin"))
            .await
            .expect("read binary")
            .is_binary()
    );
}

#[tokio::test]
async fn oversized_untracked_content_is_omitted_from_snapshots() {
    let repo = Repo::init();
    let large = fs::File::create(repo.root.join("large.bin")).expect("create large fixture");
    large.set_len(9 * 1024 * 1024).expect("size large fixture");

    let document = repo
        .repository()
        .await
        .snapshot(DiffScope::Unstaged)
        .await
        .expect("snapshot");
    let large = file(&document, "large.bin");
    assert!(large.binary);
    assert_eq!(large.omitted_bytes, Some(9 * 1024 * 1024));
    assert!(large.hunks.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn snapshot_rejects_non_utf8_repository_paths() {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

    let repo = Repo::init();
    let Ok(()) = fs::write(
        repo.root.join(OsStr::from_bytes(b"invalid-\xff")),
        "content\n",
    ) else {
        // Some filesystems (including common macOS volumes) reject non-UTF-8
        // names before Git can observe them.
        return;
    };

    assert!(matches!(
        repo.repository().await.snapshot(DiffScope::Unstaged).await,
        Err(GitError::Diff(
            diff_core::DiffError::UnsupportedPathEncoding(_)
        ))
    ));
}

#[tokio::test]
async fn stage_unstage_commit_and_empty_message_contracts() {
    let repo = Repo::init();
    repo.write("file.txt", "one\n");
    repo.commit_all();
    repo.write("file.txt", "two\n");
    repo.write("new.txt", "new\n");
    let repository = repo.repository().await;

    repository
        .stage(&[path("file.txt")])
        .await
        .expect("stage selected");
    assert_eq!(
        file(
            &repository
                .snapshot(DiffScope::Both)
                .await
                .expect("snapshot"),
            "file.txt"
        )
        .staged,
        StageState::Staged
    );
    repository
        .unstage(&[path("file.txt")])
        .await
        .expect("unstage selected");
    assert_eq!(
        file(
            &repository
                .snapshot(DiffScope::Both)
                .await
                .expect("snapshot"),
            "file.txt"
        )
        .staged,
        StageState::Unstaged
    );
    assert!(matches!(
        repository.commit("  \n").await,
        Err(GitError::EmptyCommitMessage)
    ));

    repository.stage_all().await.expect("stage all");
    repository.commit("update").await.expect("commit");
    assert!(
        repository
            .snapshot(DiffScope::Both)
            .await
            .expect("clean snapshot")
            .files
            .is_empty()
    );
}

#[tokio::test]
async fn snapshots_cover_renames_copies_deletions_binary_and_mode_changes() {
    let repo = Repo::init();
    repo.write("old.rs", "fn kept() {}\n");
    repo.write("source.txt", "copy me\n");
    repo.write("delete.txt", "remove me\n");
    repo.write("binary.bin", [0, 1, 2]);
    repo.write("script.sh", "#!/bin/sh\n");
    repo.commit_all();

    repo.git(&["mv", "old.rs", "new.rs"]);
    fs::copy(repo.root.join("source.txt"), repo.root.join("copy.txt")).expect("copy fixture");
    repo.git(&["add", "copy.txt"]);
    repo.remove("delete.txt");
    repo.write("binary.bin", [0, 255, 3]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(repo.root.join("script.sh"))
            .expect("metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(repo.root.join("script.sh"), permissions).expect("chmod fixture");
    }

    let document = repo
        .repository()
        .await
        .snapshot(DiffScope::Both)
        .await
        .expect("snapshot");
    assert_eq!(file(&document, "new.rs").status, FileStatus::Renamed);
    assert_eq!(file(&document, "copy.txt").status, FileStatus::Copied);
    assert_eq!(file(&document, "delete.txt").status, FileStatus::Deleted);
    assert!(file(&document, "binary.bin").binary);
    #[cfg(unix)]
    assert!(file(&document, "script.sh").mode.is_some());
}

#[tokio::test]
async fn discard_restores_tracked_files_and_removes_untracked_files() {
    let repo = Repo::init();
    repo.write("modified.txt", "original\n");
    repo.write("deleted.txt", "restore\n");
    repo.commit_all();
    repo.write("modified.txt", "changed\n");
    repo.remove("deleted.txt");
    repo.write("rename-me.txt", "rename\n");
    repo.git(&["add", "rename-me.txt"]);
    repo.git(&["commit", "-m", "rename fixture"]);
    repo.git(&["mv", "rename-me.txt", "renamed.txt"]);
    repo.write("untracked.txt", "scratch\n");
    let repository = repo.repository().await;

    repository
        .discard(&path("modified.txt"), FileStatus::Modified)
        .await
        .expect("discard modification");
    repository
        .discard(&path("deleted.txt"), FileStatus::Deleted)
        .await
        .expect("discard deletion");
    repository
        .discard(&path("untracked.txt"), FileStatus::Untracked)
        .await
        .expect("discard untracked");
    repository
        .discard(&path("renamed.txt"), FileStatus::Renamed)
        .await
        .expect("discard rename");

    assert_eq!(
        fs::read(repo.root.join("modified.txt")).expect("read"),
        b"original\n"
    );
    assert_eq!(
        fs::read(repo.root.join("deleted.txt")).expect("read"),
        b"restore\n"
    );
    assert!(!repo.root.join("untracked.txt").exists());
    assert!(repo.root.join("rename-me.txt").exists());
    assert!(!repo.root.join("renamed.txt").exists());
    assert!(
        repository
            .snapshot(DiffScope::Both)
            .await
            .expect("clean snapshot")
            .files
            .is_empty()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn full_file_reads_reject_symlinks_that_escape_the_repository() {
    use std::os::unix::fs::symlink;

    let repo = Repo::init();
    let outside = TempDir::new().expect("outside directory");
    fs::write(outside.path().join("secret"), "secret\n").expect("write secret");
    symlink(outside.path().join("secret"), repo.root.join("link")).expect("create symlink");

    assert!(matches!(
        repo.repository()
            .await
            .read_worktree_file(&path("link"))
            .await,
        Err(GitError::PathEscapesRepository { .. })
    ));
}
