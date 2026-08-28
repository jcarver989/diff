//! Native, asynchronous Git repository operations for diff review.
//!
//! This crate shells out to the host's `git` executable. It is intentionally
//! separate from `diff-core` and is not available in browser builds.

mod command;
mod error;
mod path;
mod repository;

pub use error::GitError;
pub use repository::{FileContent, GitRepository};
