//! Ratatui reviewer for rendered Markdown documents.

mod input;
mod layout;
mod render;
mod state;

pub use clankerdiff_markdown::MarkdownReviewEvent;
pub use input::handle_crossterm_event;
pub use render::MarkdownReviewWidget;
pub use state::{MarkdownFocusPane, MarkdownReviewState};
