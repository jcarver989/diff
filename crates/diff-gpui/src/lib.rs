//! A reusable, renderer-only GPUI diff review component.
//!
//! [`DiffViewer`] accepts an immutable [`clankerdiff_core::DiffDocument`] snapshot and
//! clipboard policy, and agent communication belong to the embedding shell.

mod annotation;
mod client_view;
mod comment_editor;
mod comments;
mod diff_preview;
mod diff_view;
mod markdown_viewer;
mod review_bar;
mod shortcuts;
mod sidebar;
pub mod style;
#[cfg(feature = "test-support")]
pub mod testing;
mod viewer;

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
pub use markdown_viewer::{
    MarkdownAddComment, MarkdownApprove, MarkdownCancel, MarkdownCopyReview, MarkdownDeleteComment,
    MarkdownEditComment, MarkdownFirstTarget, MarkdownLastTarget, MarkdownNextHeading,
    MarkdownNextTarget, MarkdownOpenSelected, MarkdownPageDown, MarkdownPageUp,
    MarkdownPreviousHeading, MarkdownPreviousTarget, MarkdownRequestChanges, MarkdownReviewer,
    MarkdownReviewerOptions, MarkdownShowShortcuts, MarkdownShowThemePicker, MarkdownSubmitComment,
    MarkdownToggleFocus, MarkdownToggleOutline, MarkdownUndoComment,
};
pub use viewer::{
    ActivateGap, AddComment, Cancel, CancelComment, CancelRepositoryPrompt, Collapse,
    CommitChanges, ConfirmDiscard, CopyReview, CycleScope, CycleViewMode, DeleteComment,
    DiffViewer, DiffViewerOptions, DiscardChanges, EditComment, ExpandGap, ExpandGapAll,
    ExpandOrOpen, FirstItem, FocusDiff, FocusFiles, HideShortcuts, LastItem, NextFile, NextHunk,
    NextItem, PageDown, PageUp, PreviousFile, PreviousHunk, PreviousItem, Refresh, SelectNewSide,
    SelectOldSide, ShowShortcuts, ShowThemePicker, StageAll, SubmitComment, SubmitReview,
    ToggleAllFilesCollapsed, ToggleFileCollapsed, ToggleFullFile, TogglePane, ToggleSidebar,
    ToggleSourceView, ToggleStage, UndoComment, UnstageAll, ViewerPane,
};
