#![allow(missing_docs)] // GPUI's `actions!` macro cannot attach per-action rustdoc.

use crate::{DiffViewerEvent, style::color};
use diff_core::{
    DiffDocument, DiffPresentation, DiffSide, DiffTheme, HighlightSpan, HighlightStats, Layout,
    LineAnchor, PresentedCell, PresentedRow, Review, ReviewSession, SessionOptions,
    SyntaxHighlighter, ViewMode,
};
use gpui::{
    App, Context, EventEmitter, KeyBinding, KeyDownEvent, Window, actions, div, prelude::*, px,
};
use std::sync::Arc;

actions!(
    diff_viewer,
    [
        NextFile,
        PreviousFile,
        NextHunk,
        PreviousHunk,
        CycleViewMode,
        AddComment,
        EditComment,
        DeleteComment,
        SubmitComment,
        CancelComment,
        CopyReview,
        SubmitReview,
        Cancel
    ]
);

/// Renderer-specific sizing and virtualization settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffViewerOptions {
    /// Width of the changed-files sidebar, in logical pixels.
    pub sidebar_width: f32,
    /// Height of every virtualized diff row, in logical pixels.
    pub row_height: f32,
    /// Diff-pane width at which automatic mode switches to split layout.
    pub auto_split_width: f32,
    /// Maximum number of syntax-highlight cache entries.
    pub highlight_cache_capacity: usize,
}

impl Default for DiffViewerOptions {
    fn default() -> Self {
        Self {
            sidebar_width: 280.0,
            row_height: 32.0,
            auto_split_width: 900.0,
            highlight_cache_capacity: 512,
        }
    }
}

/// Shared GPUI diff review view.
///
pub struct DiffViewer {
    session: ReviewSession,
    theme: DiffTheme,
    highlighter: SyntaxHighlighter,
    options: DiffViewerOptions,
}

impl DiffViewer {
    /// Creates a viewer with the default Sage theme and options.
    #[must_use]
    pub fn new(document: Arc<DiffDocument>) -> Self {
        Self::with_options(document, DiffTheme::default(), DiffViewerOptions::default())
    }

    /// Creates a viewer with explicit theme and adapter options.
    #[must_use]
    pub fn with_options(
        document: Arc<DiffDocument>,
        theme: DiffTheme,
        options: DiffViewerOptions,
    ) -> Self {
        Self {
            session: ReviewSession::with_options(
                document,
                SessionOptions {
                    include_file_headers: false,
                },
            ),
            theme,
            highlighter: SyntaxHighlighter::new(options.highlight_cache_capacity),
            options,
        }
    }

    /// Installs the actions' default keyboard shortcuts on an application.
    pub fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("down", NextHunk, Some("DiffViewer")),
            KeyBinding::new("up", PreviousHunk, Some("DiffViewer")),
            KeyBinding::new("cmd-]", NextFile, Some("DiffViewer")),
            KeyBinding::new("cmd-[", PreviousFile, Some("DiffViewer")),
            KeyBinding::new("cmd-enter", SubmitComment, Some("DiffViewer")),
            KeyBinding::new("ctrl-enter", SubmitComment, Some("DiffViewer")),
            KeyBinding::new("escape", CancelComment, Some("DiffViewer")),
            KeyBinding::new("cmd-shift-c", CopyReview, Some("DiffViewer")),
            KeyBinding::new("cmd-shift-enter", SubmitReview, Some("DiffViewer")),
        ]);
    }

    #[must_use]
    pub const fn session(&self) -> &ReviewSession {
        &self.session
    }

    pub const fn session_mut(&mut self) -> &mut ReviewSession {
        &mut self.session
    }

    /// Returns the current immutable snapshot.
    #[must_use]
    pub const fn document(&self) -> &Arc<DiffDocument> {
        self.session.document()
    }

    /// Returns the shared neutral theme.
    #[must_use]
    pub const fn theme(&self) -> &DiffTheme {
        &self.theme
    }

    /// Returns the structured review.
    #[must_use]
    pub const fn review(&self) -> &Review {
        self.session.review()
    }

    /// Returns the indexed renderer-neutral presentation.
    #[must_use]
    pub const fn presentation(&self) -> &DiffPresentation {
        self.session.presentation()
    }

    /// Returns the current requested view mode.
    #[must_use]
    pub const fn view_mode(&self) -> ViewMode {
        self.session.view_mode()
    }

    #[must_use]
    pub const fn layout(&self) -> Layout {
        self.session.layout()
    }

    #[must_use]
    pub fn selected_file(&self) -> Option<usize> {
        self.session.selected_file()
    }

    /// Returns highlighter cache/work counters.
    #[must_use]
    pub const fn highlight_stats(&self) -> HighlightStats {
        self.highlighter.stats()
    }

    /// Replaces the document while preserving file selection and reconciling comments.
    pub fn set_document(&mut self, document: Arc<DiffDocument>, cx: &mut Context<Self>) {
        self.session.set_document(document);
        cx.notify();
    }

    /// Changes the theme and invalidates cached syntax spans.
    pub fn set_theme(&mut self, theme: DiffTheme, cx: &mut Context<Self>) {
        self.theme = theme;
        self.highlighter.clear_cache();
        cx.notify();
    }

    /// Selects automatic, unified, or split layout.
    pub fn set_view_mode(&mut self, mode: ViewMode, cx: &mut Context<Self>) {
        if self.session.set_view_mode(mode) {
            cx.notify();
        }
    }

    /// Clears queued comments and the active draft.
    pub fn clear_review(&mut self, cx: &mut Context<Self>) {
        self.session.clear_review();
        cx.notify();
    }

    /// Adds a comment without requiring simulated UI input.
    pub fn add_comment(
        &mut self,
        anchor: LineAnchor,
        line_text: impl Into<String>,
        body: impl Into<String>,
        cx: &mut Context<Self>,
    ) -> u64 {
        let id = self
            .session
            .review_mut()
            .add_comment_with_context(anchor, line_text, body);
        cx.notify();
        id
    }

    /// Edits a queued comment.
    pub fn edit_comment(
        &mut self,
        id: u64,
        body: impl Into<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        let edited = self.session.review_mut().edit_comment(id, body);
        if edited {
            cx.notify();
        }
        edited
    }

    /// Deletes a queued comment.
    pub fn remove_comment(&mut self, id: u64, cx: &mut Context<Self>) -> bool {
        let removed = self.session.review_mut().remove_comment(id).is_some();
        if removed {
            cx.notify();
        }
        removed
    }

    pub(crate) const fn options(&self) -> &DiffViewerOptions {
        &self.options
    }

    pub(crate) fn select_file(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.session.select_file(index) {
            cx.notify();
        }
    }

    pub(crate) fn begin_comment(
        &mut self,
        row_index: usize,
        side: DiffSide,
        cx: &mut Context<Self>,
    ) {
        if self.session.select_row(row_index) {
            self.session.set_selected_side(side);
            self.session.begin_draft(None);
            cx.notify();
        }
    }

    pub(crate) fn highlight_cell(
        &mut self,
        row: &PresentedRow,
        cell: &PresentedCell,
    ) -> Vec<HighlightSpan> {
        self.session
            .presentation()
            .highlight_cell(&mut self.highlighter, &self.theme, row, cell)
    }

    fn move_file(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.session.move_file(delta).is_some() {
            cx.notify();
        }
    }

    fn move_hunk(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.session.move_hunk(delta) {
            cx.notify();
        }
    }

    fn next_file(&mut self, _: &NextFile, _: &mut Window, cx: &mut Context<Self>) {
        self.move_file(1, cx);
    }

    fn previous_file(&mut self, _: &PreviousFile, _: &mut Window, cx: &mut Context<Self>) {
        self.move_file(-1, cx);
    }

    fn next_hunk(&mut self, _: &NextHunk, _: &mut Window, cx: &mut Context<Self>) {
        self.move_hunk(1, cx);
    }

    fn previous_hunk(&mut self, _: &PreviousHunk, _: &mut Window, cx: &mut Context<Self>) {
        self.move_hunk(-1, cx);
    }

    fn cycle_view_mode(&mut self, _: &CycleViewMode, _: &mut Window, cx: &mut Context<Self>) {
        if self.session.cycle_view_mode() {
            cx.notify();
        }
    }

    fn add_comment_action(&mut self, _: &AddComment, _: &mut Window, cx: &mut Context<Self>) {
        if self.session.begin_draft(None) {
            cx.notify();
        }
    }

    fn edit_comment_action(&mut self, _: &EditComment, _: &mut Window, cx: &mut Context<Self>) {
        let editing = self.session.comment_id_at_selection();
        if editing.is_some() && self.session.begin_draft(editing) {
            cx.notify();
        }
    }

    fn delete_comment_action(&mut self, _: &DeleteComment, _: &mut Window, cx: &mut Context<Self>) {
        if self.session.delete_comment_at_selection() {
            cx.notify();
        }
    }

    fn on_draft_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.session.draft_mut() else {
            return;
        };
        if event.keystroke.key == "backspace" {
            draft.delete_before_cursor();
            cx.notify();
            return;
        }
        if event.keystroke.modifiers.control
            || event.keystroke.modifiers.platform
            || event.keystroke.modifiers.function
        {
            return;
        }
        if let Some(text) = &event.keystroke.key_char {
            draft.insert(text);
            cx.notify();
        }
    }

    fn submit_comment(&mut self, _: &SubmitComment, _: &mut Window, cx: &mut Context<Self>) {
        self.session.submit_draft();
        cx.notify();
    }

    fn cancel_comment(&mut self, _: &CancelComment, _: &mut Window, cx: &mut Context<Self>) {
        self.session.cancel_draft();
        cx.notify();
    }

    pub(crate) fn copy_review(&mut self, _: &CopyReview, _: &mut Window, cx: &mut Context<Self>) {
        if !self.session.review().is_empty() {
            cx.emit(DiffViewerEvent::CopyFormattedReview(
                self.session.submission().formatted,
            ));
        }
    }

    pub(crate) fn submit_review(
        &mut self,
        _: &SubmitReview,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.emit(DiffViewerEvent::SubmitReview(self.session.submission()));
    }

    #[expect(
        clippy::unused_self,
        reason = "GPUI action handlers must take the entity as their receiver"
    )]
    pub(crate) fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DiffViewerEvent::Cancel);
    }
}

impl EventEmitter<DiffViewerEvent> for DiffViewer {}

impl Render for DiffViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width = f32::from(window.viewport_size().width) - self.options.sidebar_width;
        self.session
            .set_split_when_auto(width >= self.options.auto_split_width);
        let palette = self.theme.palette().clone();
        let draft_line = self.session.draft().map(|draft| {
            format!(
                "Comment on {} · type, then press ⌘/Ctrl+Enter: {}▏",
                draft.anchor().path,
                draft.body()
            )
        });
        div()
            .key_context("DiffViewer")
            .on_action(cx.listener(Self::next_file))
            .on_action(cx.listener(Self::previous_file))
            .on_action(cx.listener(Self::next_hunk))
            .on_action(cx.listener(Self::previous_hunk))
            .on_action(cx.listener(Self::cycle_view_mode))
            .on_action(cx.listener(Self::add_comment_action))
            .on_action(cx.listener(Self::edit_comment_action))
            .on_action(cx.listener(Self::delete_comment_action))
            .on_action(cx.listener(Self::submit_comment))
            .on_action(cx.listener(Self::cancel_comment))
            .on_action(cx.listener(Self::copy_review))
            .on_action(cx.listener(Self::submit_review))
            .on_action(cx.listener(Self::cancel))
            .on_key_down(cx.listener(Self::on_draft_key))
            .size_full()
            .flex()
            .flex_col()
            .bg(color(palette.background))
            .text_color(color(palette.foreground))
            .font_family("Lilex")
            .text_size(px(13.0))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .flex()
                    .child(self.render_sidebar(cx))
                    .child(self.render_diff(cx)),
            )
            .when_some(draft_line, |root, line| {
                root.child(
                    div()
                        .h(px(42.0))
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .border_t_1()
                        .border_color(color(palette.border))
                        .text_color(color(palette.muted))
                        .child(line),
                )
            })
            .child(self.render_review_bar(cx))
    }
}
