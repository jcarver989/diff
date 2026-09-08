use diff_git::GitError;
use std::path::PathBuf;

/// A failure while starting, watching, or loading for a [`crate::RepositoryWatcher`].
#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error(transparent)]
    Git(#[from] GitError),
    /// The platform watcher could not be created or could not observe a path.
    #[error("could not watch {path}")]
    Watch {
        /// The path the watcher was asked to observe.
        path: PathBuf,
        /// The platform watcher error.
        #[source]
        source: notify::Error,
    },
    /// The watcher task is no longer running.
    #[error("the repository watcher has stopped")]
    Stopped,
}
