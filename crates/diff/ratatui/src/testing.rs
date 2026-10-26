//! Reusable test and benchmark support.

mod backend;
mod buffer;
mod input;
mod markdown;
mod markdown_review;
mod review;

pub use backend::{BackendStats, CountingBackend};
pub use buffer::{buffer_row_text, buffer_text};
pub use input::{key, key_with, mouse};
pub use markdown::MarkdownStreamFixture;
pub use markdown_review::*;
pub use review::*;
