//! An embeddable, repository-agnostic Ratatui diff review widget.
//! Hosts provide [`clankerdiff_core::DiffDocument`] snapshots and route emitted
//! [`clankerdiff_core::DiffReviewEvent`] values. This crate never executes Git.

mod annotation;
mod annotation_layout;
mod color;
mod commands;
#[cfg(feature = "crossterm-backend")]
mod crossterm_adapter;
mod diff_commands;
mod diff_preview;
mod drawer;
mod input;
mod interaction;
mod keybindings;
mod markdown;
mod markdown_layout;
mod markdown_review;
mod options;
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

pub use clankerdiff_core::ReviewCapabilities;
pub use clankerdiff_markdown::{MarkdownDocument, MarkdownReviewError};
pub use clankerdiff_theme::ReviewTheme;
pub use color::{composite_color, layered_style, page_color};
pub use commands::{DiffReviewCommand, MarkdownReviewCommand, ReviewCommand};
pub use diff_preview::{
    DiffPreviewOptions, DiffPreviewState, DiffPreviewStats, render_diff_preview,
};
#[cfg(feature = "crossterm-backend")]
pub use input::handle_crossterm_event;
pub use interaction::{
    InputOutcome, InteractionPhase, KeyCode, KeyCode as Key, KeyEvent, KeyModifiers, MouseButton,
    MouseEvent, MouseEventKind, ReviewInput,
};
pub use keybindings::{
    BindingScope, KeyBinding, default_diff_keybindings, default_markdown_keybindings,
};
pub use markdown::{MarkdownRenderStats, MarkdownRenderer, StreamingMarkdownState};
pub use markdown_layout::{
    MarkdownLayout, MarkdownLayoutOptions, MarkdownPresentation, MarkdownRow, MarkdownRowUpdate,
    MarkdownRows,
};
#[cfg(feature = "crossterm-backend")]
pub use markdown_review::handle_crossterm_event as handle_markdown_crossterm_event;
pub use markdown_review::{
    MarkdownFocusPane, MarkdownReview, MarkdownReviewBuilder, MarkdownReviewEvent,
    MarkdownReviewState, MarkdownReviewWidget,
};
pub use options::{NavigationPane, ReviewOptions};
pub use render::DiffReviewWidget;
pub use state::{DiffReviewState, DiffReviewStatus, FocusPane, RepositoryOperationStatus};
pub use style::{RatatuiTheme, RatatuiUiTheme};
pub use syntax::highlighted_line;
pub use theme_picker::ThemeChoice;

/// Review event emitted to the embedding host.
pub type DiffReviewEvent = clankerdiff_core::DiffReviewEvent;
