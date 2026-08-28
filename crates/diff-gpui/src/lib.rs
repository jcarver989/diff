//! A reusable, renderer-only GPUI diff review component.
//!
//! [`DiffViewer`] accepts an immutable [`diff_core::DiffDocument`] snapshot and
//! clipboard policy, and agent communication belong to the embedding shell.

mod diff_view;
mod review_bar;
mod sidebar;
pub mod style;
mod viewer;

pub use diff_core::DiffReviewEvent as DiffViewerEvent;
pub use viewer::{
    AddComment, Cancel, CancelComment, CopyReview, CycleViewMode, DeleteComment, DiffViewer,
    DiffViewerOptions, EditComment, NextFile, NextHunk, PreviousFile, PreviousHunk, SubmitComment,
    SubmitReview,
};
