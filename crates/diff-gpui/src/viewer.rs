#![allow(missing_docs)] // GPUI's `actions!` macro cannot attach per-action rustdoc.

use crate::{
    DiffViewerEvent,
    comment_editor::{CommentEditor, CommentEditorEvent},
    style::color,
};
use diff_core::{
    DiffDocument, DiffPresentation, DiffSide, DiffTheme, HighlightSpan, HighlightStats, Layout,
    LineAnchor, PresentedCell, PresentedRow, Review, ReviewSession, SessionOptions,
    SyntaxHighlighter, ViewMode,
};
use gpui::{
    App, Context, Entity, EventEmitter, Focusable, KeyBinding, ListAlignment, ListState,
    Subscription, Window, actions, div, prelude::*, px,
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
    /// Minimum height of a diff row, in logical pixels.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommentTarget {
    pub(crate) row_index: usize,
    pub(crate) side: DiffSide,
}

/// Shared GPUI diff review view.
///
pub struct DiffViewer {
    session: ReviewSession,
    theme: DiffTheme,
    highlighter: SyntaxHighlighter,
    options: DiffViewerOptions,
    diff_list_state: ListState,
    diff_list_file: Option<usize>,
    diff_list_split: bool,
    pub(crate) comment_target: Option<CommentTarget>,
    pub(crate) comment_editor: Option<Entity<CommentEditor>>,
    comment_editor_subscription: Option<Subscription>,
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
            diff_list_state: ListState::new(0, ListAlignment::Top, px(options.row_height * 8.0)),
            diff_list_file: None,
            diff_list_split: false,
            comment_target: None,
            comment_editor: None,
            comment_editor_subscription: None,
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
        self.clear_comment_editor();
        self.diff_list_file = None;
        cx.notify();
    }

    /// Changes the theme and invalidates cached syntax spans.
    pub fn set_theme(&mut self, theme: DiffTheme, cx: &mut Context<Self>) {
        self.theme = theme.clone();
        if let Some(editor) = &self.comment_editor {
            editor.update(cx, |editor, cx| editor.set_theme(theme, cx));
        }
        self.highlighter.clear_cache();
        self.diff_list_state.remeasure();
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
        self.clear_comment_editor();
        self.diff_list_state.remeasure();
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
        self.diff_list_state.remeasure();
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
            self.diff_list_state.remeasure();
            cx.notify();
        }
        edited
    }

    /// Deletes a queued comment.
    pub fn remove_comment(&mut self, id: u64, cx: &mut Context<Self>) -> bool {
        let removed = self.session.review_mut().remove_comment(id).is_some();
        if removed {
            self.diff_list_state.remeasure();
            cx.notify();
        }
        removed
    }

    pub(crate) const fn options(&self) -> &DiffViewerOptions {
        &self.options
    }

    pub(crate) fn sync_diff_list(
        &mut self,
        file_index: usize,
        row_count: usize,
        split: bool,
    ) -> ListState {
        if self.diff_list_file != Some(file_index) || self.diff_list_state.item_count() != row_count
        {
            self.diff_list_state
                .reset_with_uniform_height(row_count, px(self.options.row_height));
            self.diff_list_file = Some(file_index);
            self.diff_list_split = split;
        } else if self.diff_list_split != split {
            self.diff_list_state.remeasure();
            self.diff_list_split = split;
        }
        self.diff_list_state.clone()
    }

    pub(crate) fn select_file(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.session.select_file(index) {
            self.clear_comment_editor();
            cx.notify();
        }
    }

    pub(crate) fn begin_comment(
        &mut self,
        row_index: usize,
        side: DiffSide,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_row = self.comment_target.as_ref().map(|target| target.row_index);
        if !self.session.select_row(row_index) || !self.session.set_selected_side(side) {
            return;
        }
        if !self.session.begin_draft(None) {
            return;
        }

        let body = self
            .session
            .draft()
            .map_or_else(String::new, |draft| draft.body().to_owned());
        let editor = cx.new(|cx| CommentEditor::new(body, self.theme.clone(), cx));
        self.comment_editor_subscription = Some(cx.subscribe(
            &editor,
            |viewer, _editor, event: &CommentEditorEvent, cx| match event {
                CommentEditorEvent::Changed(body) => {
                    if let Some(draft) = viewer.session.draft_mut() {
                        draft.set_body(body);
                    }
                    viewer.remeasure_comment_row();
                    cx.notify();
                }
                CommentEditorEvent::Submit => viewer.finish_comment(cx),
                CommentEditorEvent::Cancel => viewer.discard_comment(cx),
            },
        ));
        self.comment_target = Some(CommentTarget { row_index, side });
        self.comment_editor = Some(editor.clone());
        self.remeasure_row(previous_row);
        self.remeasure_row(Some(row_index));
        editor.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn remeasure_row(&self, row_index: Option<usize>) {
        let (Some(row_index), Some(file_index)) = (row_index, self.selected_file()) else {
            return;
        };
        let Some(range) = self.presentation().file_range(file_index) else {
            return;
        };
        if range.contains(&row_index) {
            let local_index = row_index - range.start;
            self.diff_list_state
                .remeasure_items(local_index..local_index + 1);
        }
    }

    fn remeasure_comment_row(&self) {
        self.remeasure_row(self.comment_target.as_ref().map(|target| target.row_index));
    }

    fn clear_comment_editor(&mut self) {
        self.comment_target = None;
        self.comment_editor = None;
        self.comment_editor_subscription = None;
    }

    pub(crate) fn finish_comment(&mut self, cx: &mut Context<Self>) {
        let row = self.comment_target.as_ref().map(|target| target.row_index);
        if let Some(editor) = &self.comment_editor
            && let Some(draft) = self.session.draft_mut()
        {
            draft.set_body(editor.read(cx).body());
        }
        self.session.submit_draft();
        self.clear_comment_editor();
        self.remeasure_row(row);
        cx.notify();
    }

    pub(crate) fn discard_comment(&mut self, cx: &mut Context<Self>) {
        let row = self.comment_target.as_ref().map(|target| target.row_index);
        self.session.cancel_draft();
        self.clear_comment_editor();
        self.remeasure_row(row);
        cx.notify();
    }

    pub(crate) fn highlight_cell(
        &mut self,
        row: &PresentedRow,
        cell: &PresentedCell,
    ) -> Arc<[HighlightSpan]> {
        self.session
            .presentation()
            .highlight_cell(&mut self.highlighter, &self.theme, row, cell)
    }

    fn move_file(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.session.move_file(delta).is_some() {
            self.clear_comment_editor();
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

    fn add_comment_action(&mut self, _: &AddComment, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(row_index) = self.session.selected_row() {
            self.begin_comment(row_index, self.session.selected_side(), window, cx);
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
            self.diff_list_state.remeasure();
            cx.notify();
        }
    }

    fn submit_comment(&mut self, _: &SubmitComment, _: &mut Window, cx: &mut Context<Self>) {
        self.finish_comment(cx);
    }

    fn cancel_comment(&mut self, _: &CancelComment, _: &mut Window, cx: &mut Context<Self>) {
        self.discard_comment(cx);
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
                    .child(self.render_diff(window, cx)),
            )
            .child(self.render_review_bar(cx))
    }
}
