//! Errors produced by native Git and filesystem operations.

use diff_core::{DiffError, RepoPathError};
use std::{io, path::PathBuf};

/// An error from repository discovery, Git execution, or worktree access.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// The supplied location is not inside a Git worktree.
    #[error("path is not inside a Git worktree")]
    NotRepository,
    /// Git returned a repository root that cannot be represented as UTF-8.
    #[error("repository root is not valid UTF-8")]
    UnsupportedRepositoryPath,
    /// A repository-relative path failed validation.
    #[error("invalid repository path: {0}")]
    InvalidPath(#[from] RepoPathError),
    /// A validated path resolved outside the repository root.
    #[error("repository path escapes the worktree: {path}")]
    PathEscapesRepository {
        /// The rejected path.
        path: String,
    },
    /// A file operation unexpectedly targeted a directory.
    #[error("repository path is not a file: {path}")]
    NotAFile {
        /// The rejected path.
        path: String,
    },
    /// A commit message was empty or only whitespace.
    #[error("commit message must not be empty")]
    EmptyCommitMessage,
    /// The Git process could not be started or awaited.
    #[error("could not execute git operation `{operation}`: {source}")]
    Spawn {
        /// A non-sensitive operation label.
        operation: &'static str,
        /// The subprocess I/O error.
        #[source]
        source: io::Error,
    },
    /// Git exited unsuccessfully.
    ///
    /// `stderr` is retained for diagnostics, but omitted from `Display` to
    /// avoid leaking file contents or other sensitive command output.
    #[error("git operation `{operation}` failed with status {status:?}")]
    CommandFailed {
        /// A non-sensitive operation label.
        operation: &'static str,
        /// The process exit code, when one was available.
        status: Option<i32>,
        /// Git's standard error bytes, decoded lossily.
        stderr: String,
    },
    /// A filesystem operation failed.
    #[error("filesystem operation failed for {path}")]
    Io {
        /// The affected host path.
        path: PathBuf,
        /// The filesystem error.
        #[source]
        source: io::Error,
    },
    /// A source version exceeded the per-file capture limit.
    #[error("source version is too large ({bytes} bytes)")]
    SourceTooLarge { bytes: u64 },
    /// Repository metadata or worktree content changed during capture.
    #[error("repository changed while capturing the snapshot")]
    UnstableSnapshot,
    /// Core diff normalization failed.
    #[error("could not normalize Git snapshot: {0}")]
    Diff(#[from] DiffError),
}
