#[allow(clippy::module_inception)]
mod file_watcher;
mod notify_file_watcher;

#[cfg(any(test, feature = "test-support"))]
mod fake_file_watcher;

pub use file_watcher::FileWatcher;
pub use notify_file_watcher::{FileWatchError, NotifyFileWatcher};

#[cfg(any(test, feature = "test-support"))]
pub use fake_file_watcher::{FakeFileWatcher, FakeFileWatcherHandle};
