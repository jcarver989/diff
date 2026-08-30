//! An embeddable, repository-agnostic Ratatui diff review widget.
//! Hosts provide [`diff_core::DiffDocument`] snapshots and route emitted
//! [`diff_core::DiffReviewEvent`] values. This crate never executes Git.

#[cfg(any(feature = "diff-review", feature = "markdown-review"))]
mod annotation;
#[cfg(feature = "diff-preview")]
mod diff_preview;
#[cfg(feature = "diff-review")]
mod drawer;
#[cfg(feature = "diff-review")]
mod input;
#[cfg(feature = "markdown")]
mod markdown;
#[cfg(feature = "markdown-review")]
mod markdown_review;
#[cfg(feature = "diff-review")]
mod patch_layout;
#[cfg(feature = "diff-review")]
mod render;
#[cfg(feature = "diff-review")]
mod state;
#[cfg(any(feature = "diff-review", feature = "markdown-review"))]
mod style;
#[cfg(feature = "syntax")]
mod syntax;
#[cfg(any(feature = "diff-review", feature = "markdown-review"))]
mod theme_picker;
#[cfg(any(feature = "diff-review", feature = "markdown-review"))]
mod widgets;

#[cfg(feature = "diff-preview")]
pub use diff_preview::{DiffPreviewOptions, render_diff_preview};
#[cfg(feature = "diff-review")]
pub use input::{DiffReviewInput, handle_crossterm_event};
#[cfg(feature = "markdown")]
pub use markdown::{MarkdownRenderOptions, MarkdownRenderer, StreamingMarkdownState};
#[cfg(feature = "markdown-review")]
pub use markdown_review::{
    MarkdownFocusPane, MarkdownReviewEvent, MarkdownReviewInput, MarkdownReviewState,
    MarkdownReviewWidget, handle_crossterm_event as handle_markdown_crossterm_event,
};
#[cfg(feature = "diff-review")]
pub use render::DiffReviewWidget;
#[cfg(feature = "diff-review")]
pub use state::{DiffReviewState, DiffReviewStatus, FocusPane, RepositoryOperationStatus};
#[cfg(any(feature = "diff-review", feature = "markdown-review"))]
pub use style::RatatuiTheme;
#[cfg(feature = "syntax")]
pub use syntax::highlighted_line;

/// Review event emitted to the embedding host.
#[cfg(feature = "diff-review")]
pub type DiffReviewEvent = diff_core::DiffReviewEvent;
