#![allow(missing_docs)] // GPUI's `actions!` macro cannot attach per-action rustdoc.

use crate::{
    DEFAULT_FONT_FAMILY, DiffViewerEvent,
    comment_editor::{CommentEditor, CommentEditorEvent},
    sidebar::{SidebarResizeDrag, SidebarTree},
    style::color,
};
use diff_core::{
    DiffDocument, DiffPresentation, DiffSide, DiffTheme, HighlightSpan, HighlightStats, Layout,
    LineAnchor, PresentedCell, PresentedRow, Review, ReviewSession, SessionOptions,
    SyntaxHighlighter, ViewMode,
};
use gpui::{
    App, Context, DragMoveEvent, Entity, EventEmitter, Focusable, KeyBinding, ListAlignment,
    ListState, Subscription, Window, actions, div, prelude::*, px,
};
use std::sync::Arc;

const MIN_SIDEBAR_WIDTH: f32 = 180.0;
const MAX_SIDEBAR_WIDTH: f32 = 600.0;
const MIN_DIFF_WIDTH: f32 = 320.0;
const SIDEBAR_DIVIDER_WIDTH: f32 = 1.0;
const MIN_FONT_SIZE: f32 = 10.0;
const MAX_FONT_SIZE: f32 = 24.0;
const FONT_SIZE_STEP: f32 = 1.0;
const DIFF_ROW_VERTICAL_SPACE: f32 = 19.0;
const SIDEBAR_ROW_VERTICAL_SPACE: f32 = 20.0;

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
        IncreaseFontSize,
        DecreaseFontSize,
        ResetFontSize,
        Cancel
    ]
);

/// Renderer-specific sizing and virtualization settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffViewerOptions {
    /// Width of the changed-files sidebar, in logical pixels.
    pub sidebar_width: f32,
    /// Initial viewer font size, in logical pixels.
    pub font_size: f32,
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
            font_size: 13.0,
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
    sidebar_width: f32,
    font_size: f32,
    diff_list_state: ListState,
    diff_list_file: Option<usize>,
    diff_list_split: bool,
    pub(crate) sidebar_tree: SidebarTree,
    pub(crate) comment_target: Option<CommentTarget>,
    pub(crate) comment_editor: Option<Entity<CommentEditor>>,
    comment_editor_subscription: Option<Subscription>,
}

impl DiffViewer {
    /// Creates a viewer with the default Ayu Dark theme and options.
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
        let sidebar_tree = SidebarTree::new(&document);
        let sidebar_width = options
            .sidebar_width
            .clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
        let font_size = clamp_font_size(options.font_size);
        let row_height = effective_diff_row_height(options.row_height, font_size);
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
            sidebar_width,
            font_size,
            diff_list_state: ListState::new(0, ListAlignment::Top, px(row_height * 8.0)),
            diff_list_file: None,
            diff_list_split: false,
            sidebar_tree,
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
            KeyBinding::new("cmd-=", IncreaseFontSize, Some("DiffViewer")),
            KeyBinding::new("cmd-+", IncreaseFontSize, Some("DiffViewer")),
            KeyBinding::new("ctrl-=", IncreaseFontSize, Some("DiffViewer")),
            KeyBinding::new("ctrl-+", IncreaseFontSize, Some("DiffViewer")),
            KeyBinding::new("cmd--", DecreaseFontSize, Some("DiffViewer")),
            KeyBinding::new("ctrl--", DecreaseFontSize, Some("DiffViewer")),
            KeyBinding::new("cmd-0", ResetFontSize, Some("DiffViewer")),
            KeyBinding::new("ctrl-0", ResetFontSize, Some("DiffViewer")),
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

    /// Returns the current changed-files sidebar width.
    #[must_use]
    pub const fn sidebar_width(&self) -> f32 {
        self.sidebar_width
    }

    /// Changes the changed-files sidebar width.
    pub fn set_sidebar_width(&mut self, width: f32, cx: &mut Context<Self>) {
        self.update_sidebar_width(width.clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH), cx);
    }

    /// Returns the current viewer font size.
    #[must_use]
    pub const fn font_size(&self) -> f32 {
        self.font_size
    }

    /// Changes the viewer font size.
    pub fn set_font_size(&mut self, size: f32, cx: &mut Context<Self>) {
        let size = clamp_font_size(size);
        if (self.font_size - size).abs() <= f32::EPSILON {
            return;
        }
        self.font_size = size;
        self.diff_list_state.remeasure();
        cx.notify();
    }

    /// Returns highlighter cache/work counters.
    #[must_use]
    pub const fn highlight_stats(&self) -> HighlightStats {
        self.highlighter.stats()
    }

    /// Replaces the document while preserving file selection and reconciling comments.
    pub fn set_document(&mut self, document: Arc<DiffDocument>, cx: &mut Context<Self>) {
        self.sidebar_tree.rebuild(&document);
        self.session.set_document(document);
        if let Some(index) = self.selected_file() {
            let document = self.session.document().clone();
            self.sidebar_tree.expand_file(&document, index);
        }
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

    pub(crate) fn diff_row_height(&self) -> f32 {
        effective_diff_row_height(self.options.row_height, self.font_size)
    }

    pub(crate) fn sidebar_row_height(&self) -> f32 {
        36.0_f32.max(self.font_size + SIDEBAR_ROW_VERTICAL_SPACE)
    }

    pub(crate) fn metadata_font_size(&self) -> f32 {
        (self.font_size - 1.0).max(MIN_FONT_SIZE)
    }

    pub(crate) fn heading_font_size(&self) -> f32 {
        (self.font_size + 1.0).min(MAX_FONT_SIZE)
    }

    pub(crate) fn adjust_font_size(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.set_font_size(self.font_size + delta, cx);
    }

    pub(crate) fn reset_font_size(&mut self, cx: &mut Context<Self>) {
        self.set_font_size(self.options.font_size, cx);
    }

    pub(crate) fn reset_sidebar_width(&mut self, cx: &mut Context<Self>) {
        self.set_sidebar_width(self.options.sidebar_width, cx);
    }

    fn resize_sidebar(
        &mut self,
        event: &DragMoveEvent<SidebarResizeDrag>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pointer = f32::from(event.event.position.x - event.bounds.left());
        let available = f32::from(event.bounds.size.width);
        self.update_sidebar_width(clamp_sidebar_width(pointer, available), cx);
    }

    fn update_sidebar_width(&mut self, width: f32, cx: &mut Context<Self>) {
        if (self.sidebar_width - width).abs() <= f32::EPSILON {
            return;
        }
        self.sidebar_width = width;
        self.diff_list_state.remeasure();
        cx.notify();
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
                .reset_with_uniform_height(row_count, px(self.diff_row_height()));
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
            let document = self.session.document().clone();
            self.sidebar_tree.expand_file(&document, index);
            self.clear_comment_editor();
            cx.notify();
        }
    }

    pub(crate) fn toggle_directory(&mut self, path: &str, cx: &mut Context<Self>) {
        self.sidebar_tree.toggle(path);
        cx.notify();
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
        let Some(current) = self.selected_file() else {
            return;
        };
        if let Some(index) = self.sidebar_tree.offset_file(current, delta) {
            self.select_file(index, cx);
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

    fn increase_font_size(&mut self, _: &IncreaseFontSize, _: &mut Window, cx: &mut Context<Self>) {
        self.adjust_font_size(FONT_SIZE_STEP, cx);
    }

    fn decrease_font_size(&mut self, _: &DecreaseFontSize, _: &mut Window, cx: &mut Context<Self>) {
        self.adjust_font_size(-FONT_SIZE_STEP, cx);
    }

    fn reset_font_size_action(
        &mut self,
        _: &ResetFontSize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reset_font_size(cx);
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

fn clamp_font_size(size: f32) -> f32 {
    size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
}

fn effective_diff_row_height(configured: f32, font_size: f32) -> f32 {
    configured.max(font_size + DIFF_ROW_VERTICAL_SPACE)
}

fn clamp_sidebar_width(width: f32, available: f32) -> f32 {
    let maximum = MAX_SIDEBAR_WIDTH.min((available - MIN_DIFF_WIDTH).max(0.0));
    if maximum < MIN_SIDEBAR_WIDTH {
        width.clamp(0.0, maximum)
    } else {
        width.clamp(MIN_SIDEBAR_WIDTH, maximum)
    }
}

impl Render for DiffViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport_width = f32::from(window.viewport_size().width);
        let sidebar_width = clamp_sidebar_width(self.sidebar_width, viewport_width);
        if (self.sidebar_width - sidebar_width).abs() > f32::EPSILON {
            self.sidebar_width = sidebar_width;
            self.diff_list_state.remeasure();
        }
        let diff_width = viewport_width - sidebar_width - SIDEBAR_DIVIDER_WIDTH;
        self.session
            .set_split_when_auto(diff_width >= self.options.auto_split_width);
        let palette = self.theme.palette().clone();
        div()
            .key_context("DiffViewer")
            .on_action(cx.listener(Self::next_file))
            .on_action(cx.listener(Self::previous_file))
            .on_action(cx.listener(Self::next_hunk))
            .on_action(cx.listener(Self::previous_hunk))
            .on_action(cx.listener(Self::cycle_view_mode))
            .on_action(cx.listener(Self::increase_font_size))
            .on_action(cx.listener(Self::decrease_font_size))
            .on_action(cx.listener(Self::reset_font_size_action))
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
            .font_family(DEFAULT_FONT_FAMILY)
            .text_size(px(self.font_size))
            .child(
                div()
                    .id("diff-viewer-content")
                    .flex_1()
                    .overflow_hidden()
                    .flex()
                    .on_drag_move::<SidebarResizeDrag>(cx.listener(Self::resize_sidebar))
                    .child(self.render_sidebar(cx))
                    .child(self.render_sidebar_resize_handle(cx))
                    .child(self.render_diff(window, cx)),
            )
            .child(self.render_review_bar(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn sidebar_width_is_clamped_to_preserve_the_diff_pane() {
        assert_close(clamp_sidebar_width(100.0, 1_000.0), 180.0);
        assert_close(clamp_sidebar_width(400.0, 1_000.0), 400.0);
        assert_close(clamp_sidebar_width(900.0, 1_000.0), 600.0);
        assert_close(clamp_sidebar_width(400.0, 600.0), 280.0);
    }

    #[test]
    fn sidebar_can_shrink_below_its_normal_minimum_in_a_narrow_viewport() {
        assert_close(clamp_sidebar_width(280.0, 450.0), 130.0);
        assert_close(clamp_sidebar_width(280.0, 300.0), 0.0);
    }

    #[test]
    fn font_size_is_clamped_to_the_supported_range() {
        assert_close(clamp_font_size(8.0), 10.0);
        assert_close(clamp_font_size(16.0), 16.0);
        assert_close(clamp_font_size(30.0), 24.0);
    }

    #[test]
    fn diff_rows_grow_to_fit_larger_fonts() {
        assert_close(effective_diff_row_height(32.0, 13.0), 32.0);
        assert_close(effective_diff_row_height(32.0, 20.0), 39.0);
    }
}
