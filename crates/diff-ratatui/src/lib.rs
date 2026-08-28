//! An embeddable, repository-agnostic Ratatui diff review widget.
//!
//! Hosts provide [`diff_core::DiffDocument`] snapshots and route emitted
//! [`diff_core::DiffReviewEvent`] values. This crate never executes Git.

mod input;
mod render;
mod state;
mod style;

pub use input::{DiffReviewInput, handle_crossterm_event};
pub use render::DiffReviewWidget;
pub use state::{DiffReviewState, DiffReviewStatus, FocusPane};
pub use style::RatatuiTheme;

/// Review event emitted to the embedding host.
pub type DiffReviewEvent = diff_core::DiffReviewEvent;
