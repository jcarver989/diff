#![allow(clippy::large_types_passed_by_value)]

mod annotation;
mod client_view;
mod comment_editor;
mod comments;
mod commits;
mod definition_card;
mod diff_preview;
mod diff_view;
mod markdown_viewer;
mod palette;
mod review_bar;
mod shortcuts;
mod sidebar;
mod style;
mod syntax_job;
mod toolbar;
mod viewer;

pub use clankerdiff_core::DiffReviewEvent as DiffViewerEvent;
pub use clankerdiff_rouge_ui as ui;
pub use client_view::DiffClientView;
pub use markdown_viewer::{MarkdownController, MarkdownHandle, MarkdownOutput, markdown_review_view};
pub use shortcuts::shortcut_label;
pub use viewer::{
    DiffViewerOptions, ReviewController, ReviewHandle, ReviewOutput, ReviewStatus, RunCommand, ScrollRequest,
    Selection, ViewerPane, diff_view, diff_view_with,
};

pub use diff_preview::PatchPreview;
