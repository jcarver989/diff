//! A reusable, renderer-only GPUI diff review component.
//!
//! [`DiffViewer`] accepts an immutable [`clankerdiff_core::DiffDocument`] snapshot and
//! clipboard policy, and agent communication belong to the embedding shell.

mod annotation;
mod client_view;
mod comment_editor;
mod comments;
mod commits;
mod definition_card;
mod diff_preview;
mod diff_view;
mod markdown_viewer;
mod navigation;
mod palette;
mod review_bar;
mod shortcuts;
mod sidebar;
pub mod style;
#[cfg(feature = "test-support")]
pub mod testing;
mod toolbar;
mod viewer;

use clankerdiff_core::ViewMode;

pub use clankerdiff_core::DiffReviewEvent as DiffViewerEvent;
pub use clankerdiff_gpui_ui as ui;
pub use clankerdiff_markdown::MarkdownReviewEvent as MarkdownReviewerEvent;
pub use client_view::{DiffClientEvent, DiffClientView};
pub use diff_preview::DiffPreviewCache;

/// Emitted when an in-app theme selection is committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeChanged {
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewModeChanged {
    pub mode: ViewMode,
}
pub use markdown_viewer::{
    MarkdownAddComment, MarkdownApprove, MarkdownCancel, MarkdownCopyReview, MarkdownDeleteComment,
    MarkdownEditComment, MarkdownFirstTarget, MarkdownLastTarget, MarkdownNextHeading, MarkdownNextTarget,
    MarkdownOpenSelected, MarkdownPageDown, MarkdownPageUp, MarkdownPreviousHeading, MarkdownPreviousTarget,
    MarkdownRequestChanges, MarkdownReviewer, MarkdownReviewerOptions, MarkdownShowShortcuts, MarkdownShowThemePicker,
    MarkdownSubmitComment, MarkdownToggleFocus, MarkdownToggleOutline, MarkdownUndoComment,
};
pub use viewer::{
    CancelComment, CancelRepositoryPrompt, ConfirmDiscard, DiffViewer, DiffViewerOptions, RunCommand, SubmitComment,
    ViewerPane,
};
