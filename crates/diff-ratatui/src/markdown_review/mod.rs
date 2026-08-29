//! Ratatui reviewer for rendered Markdown documents.

mod input;
mod layout;
mod render;
mod state;

pub use diff_core::MarkdownReviewEvent;
pub use input::{MarkdownReviewInput, handle_crossterm_event};
pub use render::MarkdownReviewWidget;
pub use state::{MarkdownFocusPane, MarkdownReviewState};
