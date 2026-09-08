//! Reusable test and benchmark support.
//!
//! Enable the `test-support` feature from integration tests and benchmarks
//! alongside the features whose support is needed. The review harness owns a
//! real Ratatui terminal and the production review state so input, rendering,
//! terminal diffing, and syntax caches are all exercised together.

mod backend;
#[cfg(feature = "markdown")]
mod markdown;
#[cfg(feature = "diff-review")]
mod review;

pub use backend::{BackendStats, CountingBackend};
#[cfg(feature = "markdown")]
pub use markdown::MarkdownStreamFixture;
#[cfg(feature = "diff-review")]
pub use review::*;
