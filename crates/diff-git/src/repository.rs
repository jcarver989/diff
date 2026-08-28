//! Concrete native Git repository service.

use crate::{GitError, command, path};
use diff_core::{
    DiffDocument, DiffScope, FileStatus, RepoPath, UntrackedFile, parse_porcelain_v1_z,
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// Bytes read from a worktree file, classified for safe text rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileContent {
    /// Valid UTF-8 content without embedded NUL bytes.
    Text(String),
    /// Binary or non-UTF-8 content.
    Binary(Vec<u8>),
}

impl FileContent {
    /// Classifies file bytes as renderer-safe text or binary content.
    #[must_use]
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        if bytes.contains(&0) {
            return Self::Binary(bytes);
        }
        match String::from_utf8(bytes) {
            Ok(text) => Self::Text(text),
            Err(error) => Self::Binary(error.into_bytes()),
        }
    }

    /// Returns the original file bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Text(text) => text.as_bytes(),
            Self::Binary(bytes) => bytes,
        }
    }

    /// Returns text content when this file was classified as text.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::Binary(_) => None,
        }
    }

    /// Returns whether the file was classified as binary.
    #[must_use]
    pub const fn is_binary(&self) -> bool {
        matches!(self, Self::Binary(_))
    }
}

/// A discovered Git worktree and its native operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRepository {
    root: PathBuf,
}

impl GitRepository {
    /// Discovers the containing Git worktree and stores its canonical root.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot run, the path is outside a worktree,
    /// or the discovered root cannot be represented safely.
    pub async fn discover(path: impl AsRef<Path>) -> Result<Self, GitError> {
        let candidate = path.as_ref();
        let output = match command::run(
            candidate,
            "discover repository",
            command::args(&["rev-parse", "--show-toplevel"]),
        )
        .await
        {
            Ok(output) => output,
            Err(GitError::CommandFailed { .. }) => return Err(GitError::NotRepository),
            Err(error) => return Err(error),
        };
        let root = parse_root(&output.stdout)?;
        let root = tokio::fs::canonicalize(&root)
            .await
            .map_err(|source| GitError::Io {
                path: root.clone(),
                source,
            })?;
        Ok(Self { root })
    }

    /// Returns the canonical worktree root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Loads a canonical snapshot for a staged, unstaged, or combined scope.
    ///
    /// # Errors
    ///
    /// Returns an error when Git execution, path validation, file reading, or
    /// diff normalization fails.
    pub async fn snapshot(&self, scope: DiffScope) -> Result<DiffDocument, GitError> {
        let has_head = scope != DiffScope::Both || self.has_head().await?;
        let diff = command::run(&self.root, "load diff", Self::diff_args(scope, has_head)).await?;
        let status = command::run(
            &self.root,
            "load status",
            command::args(&["status", "--porcelain=v1", "-z", "--untracked-files=all"]),
        )
        .await?;
        let untracked = if scope == DiffScope::Staged {
            Vec::new()
        } else {
            self.read_untracked().await?
        };
        let repo_root = self
            .root
            .to_str()
            .ok_or(GitError::UnsupportedRepositoryPath)?;
        DiffDocument::from_git_outputs_with_untracked(
            repo_root,
            &diff.stdout,
            &status.stdout,
            scope,
            &untracked,
        )
        .map_err(GitError::from)
    }

    /// Reads and classifies a complete worktree file.
    ///
    /// # Errors
    ///
    /// Returns an error when the path is invalid, missing, not a file, escapes
    /// through a symlink, or cannot be read.
    pub async fn read_worktree_file(&self, path: &RepoPath) -> Result<FileContent, GitError> {
        let host_path = path::readable_path(&self.root, path).await?;
        let bytes = tokio::fs::read(&host_path)
            .await
            .map_err(|source| GitError::Io {
                path: host_path,
                source,
            })?;
        Ok(FileContent::from_bytes(bytes))
    }

    /// Stages selected paths. An empty slice is a no-op.
    ///
    /// # Errors
    ///
    /// Returns an error when a path is invalid or Git cannot stage it.
    pub async fn stage(&self, paths: &[RepoPath]) -> Result<(), GitError> {
        if paths.is_empty() {
            return Ok(());
        }
        self.run_paths("stage paths", &["add"], paths).await
    }

    /// Unstages selected paths while preserving their worktree contents.
    /// An empty slice is a no-op.
    ///
    /// # Errors
    ///
    /// Returns an error when a path is invalid or Git cannot update the index.
    pub async fn unstage(&self, paths: &[RepoPath]) -> Result<(), GitError> {
        if paths.is_empty() {
            return Ok(());
        }
        if self.has_head().await? {
            self.run_paths("unstage paths", &["reset", "--quiet", "HEAD"], paths)
                .await
        } else {
            self.run_paths(
                "unstage paths",
                &["rm", "--cached", "-f", "--quiet", "--ignore-unmatch"],
                paths,
            )
            .await
        }
    }

    /// Stages all tracked, untracked, and deleted paths.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot update the index.
    pub async fn stage_all(&self) -> Result<(), GitError> {
        command::run(&self.root, "stage all", command::args(&["add", "-A", "--"]))
            .await
            .map(drop)
    }

    /// Unstages the complete index while preserving worktree contents.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot update the index.
    pub async fn unstage_all(&self) -> Result<(), GitError> {
        if self.has_head().await? {
            command::run(
                &self.root,
                "unstage all",
                command::args(&["reset", "--quiet", "HEAD", "--"]),
            )
            .await
            .map(drop)
        } else {
            command::run(
                &self.root,
                "unstage all",
                command::args(&[
                    "rm",
                    "--cached",
                    "-r",
                    "-f",
                    "--quiet",
                    "--ignore-unmatch",
                    "--",
                    ".",
                ]),
            )
            .await
            .map(drop)
        }
    }

    /// Commits the current index with the supplied non-empty message.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::EmptyCommitMessage`] for a blank message, or an
    /// execution error when Git cannot create the commit.
    pub async fn commit(&self, message: &str) -> Result<(), GitError> {
        if message.trim().is_empty() {
            return Err(GitError::EmptyCommitMessage);
        }
        let args = vec![
            OsString::from("commit"),
            OsString::from("-m"),
            OsString::from(message),
        ];
        command::run(&self.root, "commit", args).await.map(drop)
    }

    /// Discards all staged and unstaged changes for one path.
    ///
    /// Untracked paths are removed with `git clean`; tracked paths are restored
    /// from `HEAD`. In an unborn repository an added path is removed from the
    /// index and worktree.
    ///
    /// # Errors
    ///
    /// Returns an error when the path is invalid, status metadata cannot be
    /// parsed, or Git cannot restore or remove the path.
    pub async fn discard(&self, path: &RepoPath, status: FileStatus) -> Result<(), GitError> {
        path::lexical_path(&self.root, path)?;
        if status == FileStatus::Untracked {
            return self
                .run_paths(
                    "discard untracked path",
                    &["clean", "-f"],
                    std::slice::from_ref(path),
                )
                .await;
        }
        if !self.has_head().await? {
            if status != FileStatus::Added {
                return Err(GitError::CommandFailed {
                    operation: "discard path",
                    status: None,
                    stderr: "cannot restore a tracked path in a repository without HEAD".to_owned(),
                });
            }
            self.run_paths(
                "discard added path",
                &["rm", "--cached", "-f", "--ignore-unmatch"],
                std::slice::from_ref(path),
            )
            .await?;
            return self
                .run_paths(
                    "discard added path",
                    &["clean", "-f"],
                    std::slice::from_ref(path),
                )
                .await;
        }
        let mut restore_paths = vec![path.clone()];
        if status == FileStatus::Renamed {
            let output = command::run(
                &self.root,
                "resolve renamed path",
                command::args(&["status", "--porcelain=v1", "-z", "--untracked-files=all"]),
            )
            .await?;
            if let Some(old_path) = parse_porcelain_v1_z(&output.stdout)?
                .into_iter()
                .find(|entry| entry.path == *path)
                .and_then(|entry| entry.old_path)
            {
                restore_paths.push(old_path);
            }
        }
        self.run_paths(
            "discard path",
            &["restore", "--source=HEAD", "--staged", "--worktree"],
            &restore_paths,
        )
        .await
    }

    async fn read_untracked(&self) -> Result<Vec<UntrackedFile>, GitError> {
        let output = command::run(
            &self.root,
            "list untracked files",
            command::args(&["ls-files", "--others", "--exclude-standard", "-z", "--"]),
        )
        .await?;
        let mut files = Vec::new();
        for raw_path in output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
        {
            let text = std::str::from_utf8(raw_path)
                .map_err(diff_core::DiffError::UnsupportedPathEncoding)?;
            let path = RepoPath::new(text)?;
            let host_path = path::readable_path(&self.root, &path).await?;
            let contents = tokio::fs::read(&host_path)
                .await
                .map_err(|source| GitError::Io {
                    path: host_path,
                    source,
                })?;
            files.push(UntrackedFile { path, contents });
        }
        Ok(files)
    }

    async fn has_head(&self) -> Result<bool, GitError> {
        match command::run(
            &self.root,
            "resolve HEAD",
            command::args(&["rev-parse", "--verify", "--quiet", "HEAD"]),
        )
        .await
        {
            Ok(_) => Ok(true),
            Err(GitError::CommandFailed {
                status: Some(1), ..
            }) => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn diff_args(scope: DiffScope, has_head: bool) -> Vec<OsString> {
        let mut args = command::args(&[
            "diff",
            "--no-ext-diff",
            "--no-color",
            "--find-renames",
            "--find-copies",
            "--find-copies-harder",
        ]);
        match scope {
            DiffScope::Unstaged => {}
            DiffScope::Staged => args.push(OsString::from("--cached")),
            DiffScope::Both => {
                args.push(OsString::from(if has_head { "HEAD" } else { EMPTY_TREE }));
            }
        }
        args.push(OsString::from("--"));
        args
    }

    async fn run_paths(
        &self,
        operation: &'static str,
        prefix: &[&str],
        paths: &[RepoPath],
    ) -> Result<(), GitError> {
        let mut args: Vec<OsString> = prefix.iter().map(OsString::from).collect();
        args.push(OsString::from("--"));
        for path_value in paths {
            path::lexical_path(&self.root, path_value)?;
            args.push(OsString::from(path_value.as_str()));
        }
        command::run(&self.root, operation, args).await.map(drop)
    }
}

fn parse_root(stdout: &[u8]) -> Result<PathBuf, GitError> {
    let stdout = stdout.strip_suffix(b"\n").unwrap_or(stdout);
    let stdout = stdout.strip_suffix(b"\r").unwrap_or(stdout);
    let root = std::str::from_utf8(stdout).map_err(|_| GitError::UnsupportedRepositoryPath)?;
    if root.is_empty() {
        return Err(GitError::NotRepository);
    }
    Ok(PathBuf::from(root))
}
