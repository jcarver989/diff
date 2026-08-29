//! An embeddable, repository-agnostic Ratatui diff review widget.
//! Hosts provide [`diff_core::DiffDocument`] snapshots and route emitted
//! [`diff_core::DiffReviewEvent`] values. This crate never executes Git.

mod annotation;
mod drawer;
mod input;
mod markdown_review;
mod patch_layout;
mod render;
mod state;
mod style;
mod theme_picker;
mod widgets;

pub use input::{DiffReviewInput, handle_crossterm_event};
pub use markdown_review::{
    MarkdownFocusPane, MarkdownReviewEvent, MarkdownReviewInput, MarkdownReviewState,
    MarkdownReviewWidget, handle_crossterm_event as handle_markdown_crossterm_event,
};
pub use render::DiffReviewWidget;
pub use state::{DiffReviewState, DiffReviewStatus, FocusPane, RepositoryOperationStatus};
pub use style::RatatuiTheme;

/// Review event emitted to the embedding host.
pub type DiffReviewEvent = diff_core::DiffReviewEvent;
