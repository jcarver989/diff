//! Native, asynchronous Git repository operations for diff review.
//!
//! This crate shells out to the host's `git` executable. It is intentionally
//! separate from `diff-core` and is not available in browser builds.

mod command;
mod error;
mod path;
mod repository;

#[cfg(feature = "test-support")]
pub mod testing;

pub use error::GitError;
pub use repository::{
    GitRepository, MAX_SOURCE_ARCHIVE_BYTES, MAX_SOURCE_FILE_BYTES, RepositorySnapshot,
};
