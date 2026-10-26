use crate::file_watcher::FileWatchError;
use clankerdiff_git::GitError;

/// A failure while starting, watching, or loading for a [`crate::RepositoryWatcher`].
#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error(transparent)]
    Git(#[from] GitError),
    /// The platform watcher could not be created or could not observe a path.
    #[error(transparent)]
    FileWatch(#[from] FileWatchError),
    /// The watcher task is no longer running.
    #[error("the repository watcher has stopped")]
    Stopped,
}
