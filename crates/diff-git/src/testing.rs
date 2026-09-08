use crate::GitRepository;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};
use tempfile::TempDir;

#[derive(Debug, Clone, Default)]
pub struct RepoFixtureBuilder {
    files: Vec<(String, Vec<u8>)>,
    ignored: Vec<String>,
    committed: bool,
}

impl RepoFixtureBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Writes a file, creating parent directories, before the optional commit.
    #[must_use]
    pub fn file(mut self, path: &str, contents: impl AsRef<[u8]>) -> Self {
        self.files
            .push((path.to_owned(), contents.as_ref().to_vec()));
        self
    }

    /// Writes `.gitignore` with one pattern per line.
    #[must_use]
    pub fn gitignore(mut self, patterns: &[&str]) -> Self {
        self.ignored = patterns
            .iter()
            .map(|pattern| (*pattern).to_owned())
            .collect();
        self
    }

    /// Commits every written file once the worktree is populated.
    #[must_use]
    pub const fn committed(mut self) -> Self {
        self.committed = true;
        self
    }

    /// Initializes the configured worktree.
    #[must_use]
    pub fn build(self) -> RepoFixture {
        let dir = TempDir::new().expect("temporary directory");
        let root = dir.path().canonicalize().expect("canonical temporary path");
        let repo = RepoFixture {
            directory: Arc::new(dir),
            shared_repository: None,
            root,
        };
        repo.git(&["init", "--initial-branch=main"]);
        repo.git(&["config", "user.name", "Diff Contract Test"]);
        repo.git(&["config", "user.email", "diff@example.com"]);
        if !self.ignored.is_empty() {
            let mut patterns = self.ignored.join("\n");
            patterns.push('\n');
            repo.write(".gitignore", patterns);
        }
        for (path, contents) in &self.files {
            repo.write(path, contents);
        }
        if self.committed {
            repo.commit_all();
        }
        repo
    }
}

/// A temporary Git worktree kept alive for the duration of a test.
#[derive(Debug)]
pub struct RepoFixture {
    directory: Arc<TempDir>,
    shared_repository: Option<Arc<TempDir>>,
    root: PathBuf,
}

impl RepoFixture {
    /// Initializes an empty worktree with committer identity configured.
    #[must_use]
    pub fn init() -> Self {
        RepoFixtureBuilder::new().build()
    }

    /// Creates a linked worktree on a new branch, retaining its shared repository.
    #[must_use]
    pub fn linked_worktree(&self, branch: &str) -> Self {
        let dir = TempDir::new().expect("temporary worktree directory");
        let root = dir.path().join("worktree");
        self.git(&[
            "worktree",
            "add",
            "-b",
            branch,
            root.to_str().expect("UTF-8 fixture path"),
        ]);
        Self {
            root: root.canonicalize().expect("canonical worktree path"),
            directory: Arc::new(dir),
            shared_repository: Some(
                self.shared_repository
                    .as_ref()
                    .unwrap_or(&self.directory)
                    .clone(),
            ),
        }
    }

    /// Writes an executable fixture, such as a Git hook.
    #[cfg(unix)]
    pub fn write_executable(&self, path: &str, contents: &str) {
        use std::os::unix::fs::PermissionsExt;
        self.write(path, contents);
        fs::set_permissions(self.root.join(path), fs::Permissions::from_mode(0o755))
            .expect("make fixture executable");
    }

    /// Returns the canonical worktree root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Runs `git` in the worktree.
    pub fn git(&self, args: &[&str]) {
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

    /// Writes a repository-relative file, creating parent directories.
    pub fn write(&self, path: &str, contents: impl AsRef<[u8]>) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().expect("file has parent")).expect("create parents");
        fs::write(path, contents).expect("write fixture");
    }

    /// Removes a repository-relative file.
    pub fn remove(&self, path: &str) {
        fs::remove_file(self.root.join(path)).expect("remove fixture");
    }

    /// Stages and commits every change in the worktree.
    pub fn commit_all(&self) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-m", "fixture"]);
    }

    /// Discovers the fixture as a [`GitRepository`].
    pub async fn repository(&self) -> GitRepository {
        GitRepository::discover(&self.root)
            .await
            .expect("discover fixture repository")
    }
}
