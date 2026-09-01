//! Debounced native repository filesystem invalidation.

use async_channel::{Receiver, Sender, TryRecvError};
use notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};
use thiserror::Error;

/// Delay used to combine editor save sequences into one invalidation.
pub const WATCH_DEBOUNCE: Duration = Duration::from_millis(225);

/// A host-neutral repository invalidation or watcher failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryWatchEvent {
    Changed,
    Error(String),
}

/// Failure to establish a native repository watch.
#[derive(Debug, Error)]
pub enum RepositoryWatchError {
    #[error("could not canonicalize repository watch root {path}: {source}")]
    Canonicalize {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not create repository watcher: {0}")]
    Create(#[source] notify::Error),
    #[error("could not watch repository root {path}: {source}")]
    Register {
        path: PathBuf,
        #[source]
        source: notify::Error,
    },
}

/// A bounded, debounced recursive watcher for one repository worktree.
pub struct RepositoryWatcher {
    receiver: Receiver<RepositoryWatchEvent>,
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl RepositoryWatcher {
    /// Starts watching the canonical repository root recursively.
    ///
    /// # Errors
    /// Returns an error when the root cannot be canonicalized or the platform
    /// watcher cannot be created or registered.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, RepositoryWatchError> {
        let requested = root.as_ref();
        let root =
            fs::canonicalize(requested).map_err(|source| RepositoryWatchError::Canonicalize {
                path: requested.to_path_buf(),
                source,
            })?;
        let (sender, receiver) = async_channel::bounded(1);
        let mut debouncer = new_debouncer(WATCH_DEBOUNCE, None, move |result| {
            forward_result(&sender, result);
        })
        .map_err(RepositoryWatchError::Create)?;
        debouncer
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|source| RepositoryWatchError::Register { path: root, source })?;
        Ok(Self {
            receiver,
            _debouncer: debouncer,
        })
    }

    /// Returns a receiver clone for asynchronous hosts.
    #[must_use]
    pub fn receiver(&self) -> Receiver<RepositoryWatchEvent> {
        self.receiver.clone()
    }

    /// Drains queued messages without blocking.
    ///
    /// The boolean is true when at least one repository change was observed.
    /// A runtime watcher error takes precedence and should disable this watcher.
    ///
    /// # Errors
    /// Returns the watcher runtime failure message.
    pub fn drain(&self) -> Result<bool, String> {
        let mut changed = false;
        loop {
            match self.receiver.try_recv() {
                Ok(RepositoryWatchEvent::Changed) => changed = true,
                Ok(RepositoryWatchEvent::Error(error)) => return Err(error),
                Err(TryRecvError::Empty | TryRecvError::Closed) => return Ok(changed),
            }
        }
    }
}

fn forward_result(sender: &Sender<RepositoryWatchEvent>, result: DebounceEventResult) {
    let event = match result {
        Ok(events) if events.iter().any(|event| should_refresh(event.kind)) => {
            Some(RepositoryWatchEvent::Changed)
        }
        Ok(_) => None,
        Err(errors) => Some(RepositoryWatchEvent::Error(
            errors
                .into_iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        )),
    };
    if let Some(event) = event {
        let _ = sender.try_send(event);
    }
}

const fn should_refresh(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::Any | EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, CreateKind, ModifyKind, RemoveKind};

    #[test]
    fn mutation_events_refresh_but_access_and_watch_noise_do_not() {
        assert!(should_refresh(EventKind::Any));
        assert!(should_refresh(EventKind::Create(CreateKind::Any)));
        assert!(should_refresh(EventKind::Modify(ModifyKind::Any)));
        assert!(should_refresh(EventKind::Remove(RemoveKind::Any)));
        assert!(!should_refresh(EventKind::Access(AccessKind::Any)));
        assert!(!should_refresh(EventKind::Other));
    }

    #[test]
    fn startup_failures_use_the_typed_error() {
        let directory = tempfile::TempDir::new().expect("temporary directory");
        let missing = directory.path().join("missing");
        let result = RepositoryWatcher::new(&missing);
        assert!(matches!(
            result,
            Err(RepositoryWatchError::Canonicalize { path, .. }) if path == missing
        ));
    }

    #[test]
    fn bounded_delivery_coalesces_pending_invalidations() {
        let (sender, receiver) = async_channel::bounded(1);
        assert!(sender.try_send(RepositoryWatchEvent::Changed).is_ok());
        assert!(sender.try_send(RepositoryWatchEvent::Changed).is_err());
        assert_eq!(receiver.try_recv(), Ok(RepositoryWatchEvent::Changed));
        assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
    }
}
