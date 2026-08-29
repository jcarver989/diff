//! A reusable, renderer-only GPUI diff review component.
//!
//! [`DiffViewer`] accepts an immutable [`diff_core::DiffDocument`] snapshot and
//! clipboard policy, and agent communication belong to the embedding shell.

mod annotation;
mod comment_editor;
mod diff_view;
mod fonts;
mod markdown_viewer;
mod review_bar;
mod shortcuts;
mod sidebar;
pub mod style;
mod viewer;

pub use diff_core::DiffReviewEvent as DiffViewerEvent;
pub use diff_core::MarkdownReviewEvent as MarkdownReviewerEvent;
pub use fonts::{DEFAULT_FONT_FAMILY, load_default_fonts};
pub use markdown_viewer::{
    MarkdownAddComment, MarkdownApprove, MarkdownCancel, MarkdownCancelComment,
    MarkdownDeleteComment, MarkdownEditComment, MarkdownFirstTarget, MarkdownLastTarget,
    MarkdownNextHeading, MarkdownNextTarget, MarkdownPreviousHeading, MarkdownPreviousTarget,
    MarkdownRequestChanges, MarkdownReviewer, MarkdownReviewerOptions, MarkdownSubmitComment,
    MarkdownUndoComment,
};
pub use viewer::{
    AddComment, Cancel, CancelComment, Collapse, CopyReview, CycleViewMode, DecreaseFontSize,
    DeleteComment, DiffViewer, DiffViewerOptions, EditComment, ExpandOrOpen, FirstItem, FocusDiff,
    FocusFiles, HideShortcuts, IncreaseFontSize, LastItem, NextFile, NextHunk, NextItem, PageDown,
    PageUp, PreviousFile, PreviousHunk, PreviousItem, ResetFontSize, SelectNewSide, SelectOldSide,
    ShowShortcuts, SubmitComment, SubmitReview, TogglePane, UndoComment, ViewerPane,
};
