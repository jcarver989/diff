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
pub mod ui;
mod viewer;

pub use diff_core::DiffReviewEvent as DiffViewerEvent;
pub use diff_markdown::MarkdownReviewEvent as MarkdownReviewerEvent;
pub use fonts::{DEFAULT_FONT_FAMILY, load_default_fonts};

/// Emitted when an in-app theme selection is committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeChanged {
    pub id: String,
}
pub use markdown_viewer::{
    MarkdownAddComment, MarkdownApprove, MarkdownCancel, MarkdownCancelComment,
    MarkdownDeleteComment, MarkdownEditComment, MarkdownFirstTarget, MarkdownHideThemePicker,
    MarkdownLastTarget, MarkdownNextHeading, MarkdownNextTarget, MarkdownPreviousHeading,
    MarkdownPreviousTarget, MarkdownRequestChanges, MarkdownReviewer, MarkdownReviewerOptions,
    MarkdownShowThemePicker, MarkdownSubmitComment, MarkdownUndoComment,
};
pub use viewer::{
    ActivateGap, AddComment, Cancel, CancelComment, CancelRepositoryPrompt, Collapse,
    CommitChanges, ConfirmDiscard, CopyReview, CycleViewMode, DecreaseFontSize, DeleteComment,
    DiffViewer, DiffViewerOptions, DiscardChanges, EditComment, ExpandGap, ExpandGapAll,
    ExpandOrOpen, FirstItem, FocusDiff, FocusFiles, HideShortcuts, HideThemePicker,
    IncreaseFontSize, LastItem, NextFile, NextHunk, NextItem, PageDown, PageUp, PreviousFile,
    PreviousHunk, PreviousItem, ResetFontSize, SelectNewSide, SelectOldSide, ShowShortcuts,
    ShowThemePicker, StageAll, SubmitComment, SubmitReview, ToggleFullFile, TogglePane,
    ToggleStage, UndoComment, UnstageAll, ViewerPane, default_font_size,
};
