//! An embeddable, repository-agnostic Ratatui diff review widget.
//! Hosts provide [`clankerdiff_core::DiffDocument`] snapshots and route emitted
//! [`clankerdiff_core::DiffReviewEvent`] values. This crate never executes Git.

mod annotation;
mod annotation_layout;
mod color;
#[cfg(feature = "crossterm-backend")]
mod crossterm_adapter;
mod diff_preview;
mod drawer;
mod input;
mod interaction;
mod markdown;
mod markdown_layout;
mod markdown_review;
mod patch_layout;
mod render;
mod state;
mod style;
mod syntax;
#[cfg(feature = "test-support")]
pub mod testing;
mod text;
mod theme_picker;
pub mod ui;
mod widgets;

pub use color::{composite_color, layered_style, page_color};
pub use diff_preview::{
    DiffPreviewOptions, DiffPreviewState, DiffPreviewStats, render_diff_preview,
};
#[cfg(feature = "crossterm-backend")]
pub use input::handle_crossterm_event;
pub use interaction::{
    InputOutcome, InteractionPhase, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind, ReviewInput,
};
pub use markdown::{MarkdownRenderStats, MarkdownRenderer, StreamingMarkdownState};
pub use markdown_layout::{
    MarkdownLayout, MarkdownLayoutOptions, MarkdownPresentation, MarkdownRow, MarkdownRowUpdate,
    MarkdownRows,
};
#[cfg(feature = "crossterm-backend")]
pub use markdown_review::handle_crossterm_event as handle_markdown_crossterm_event;
pub use markdown_review::{
    MarkdownFocusPane, MarkdownReviewEvent, MarkdownReviewState, MarkdownReviewWidget,
};
pub use render::DiffReviewWidget;
pub use state::{DiffReviewState, DiffReviewStatus, FocusPane, RepositoryOperationStatus};
pub use style::{RatatuiTheme, RatatuiUiTheme};
pub use syntax::highlighted_line;

/// Review event emitted to the embedding host.
pub type DiffReviewEvent = clankerdiff_core::DiffReviewEvent;
