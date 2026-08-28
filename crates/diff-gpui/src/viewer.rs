#![allow(missing_docs)] // GPUI's `actions!` macro cannot attach per-action rustdoc.

use crate::{DiffViewerEvent, comment_input::CommentDraft, style::color};
use diff_core::{
    DiffDocument, DiffPresentation, DiffTheme, HighlightSpan, HighlightStats, LineAnchor,
    PresentationOptions, Review, SyntaxHighlighter, ViewMode,
};
use gpui::{
    App, Context, EventEmitter, KeyBinding, KeyDownEvent, Window, actions, div, prelude::*, px,
};
use std::{ops::Range, sync::Arc};

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
/// This entity has no repository, process, filesystem, window-creation, or
/// agent-runtime responsibilities. Embed it in a native or web shell and
/// observe [`DiffViewerEvent`] values with GPUI's event subscription API.
pub struct DiffViewer {
    document: Arc<DiffDocument>,
    presentation: DiffPresentation,
    theme: DiffTheme,
    highlighter: SyntaxHighlighter,
    review: Review,
    options: DiffViewerOptions,
    selected_file: usize,
    selected_row: usize,
    requested_mode: ViewMode,
    split_when_auto: bool,
    draft: Option<CommentDraft>,
    last_visible_range: Range<usize>,
    rendered_visible_rows: usize,
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
        let requested_mode = ViewMode::Auto;
        let presentation = build_presentation(document.clone(), requested_mode, false);
        Self {
            document,
            presentation,
            theme,
            highlighter: SyntaxHighlighter::new(options.highlight_cache_capacity),
            review: Review::default(),
            options,
            selected_file: 0,
            selected_row: 0,
            requested_mode,
            split_when_auto: false,
            draft: None,
            last_visible_range: 0..0,
            rendered_visible_rows: 0,
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

    /// Returns the current immutable snapshot.
    #[must_use]
    pub const fn document(&self) -> &Arc<DiffDocument> {
        &self.document
    }

    /// Returns the shared neutral theme.
    #[must_use]
    pub const fn theme(&self) -> &DiffTheme {
        &self.theme
    }

    /// Returns the structured review.
    #[must_use]
    pub const fn review(&self) -> &Review {
        &self.review
    }

    /// Returns the indexed renderer-neutral presentation.
    #[must_use]
    pub const fn presentation(&self) -> &DiffPresentation {
        &self.presentation
    }

    /// Returns the current requested view mode.
    #[must_use]
    pub const fn view_mode(&self) -> ViewMode {
        self.requested_mode
    }

    /// Returns the selected file index when there are changed files.
    #[must_use]
    pub fn selected_file(&self) -> Option<usize> {
        (!self.document.files.is_empty()).then_some(self.selected_file)
    }

    /// Returns the most recent range requested by GPUI's uniform list.
    #[must_use]
    pub fn last_visible_range(&self) -> Range<usize> {
        self.last_visible_range.clone()
    }

    /// Returns the number of rows converted during the latest visible request.
    #[must_use]
    pub const fn rendered_visible_rows(&self) -> usize {
        self.rendered_visible_rows
    }

    /// Returns highlighter cache/work counters.
    #[must_use]
    pub fn highlight_stats(&self) -> HighlightStats {
        self.highlighter.stats()
    }

    /// Replaces the document while preserving file selection and reconciling comments.
    pub fn set_document(&mut self, document: Arc<DiffDocument>, cx: &mut Context<Self>) {
        let selected_path = self
            .document
            .files
            .get(self.selected_file)
            .map(|file| file.path.clone());
        self.review.reconcile(&document);
        self.selected_file = selected_path
            .and_then(|path| document.files.iter().position(|file| file.path == path))
            .unwrap_or(0)
            .min(document.files.len().saturating_sub(1));
        self.document = document;
        self.draft = None;
        self.rebuild_presentation();
        self.select_first_row();
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
        if self.requested_mode != mode {
            self.requested_mode = mode;
            self.rebuild_presentation();
            self.select_first_row();
            cx.notify();
        }
    }

    /// Clears queued comments and the active draft.
    pub fn clear_review(&mut self, cx: &mut Context<Self>) {
        self.review.clear();
        self.draft = None;
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
            .review
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
        let edited = self.review.edit_comment(id, body);
        if edited {
            cx.notify();
        }
        edited
    }

    /// Deletes a queued comment.
    pub fn remove_comment(&mut self, id: u64, cx: &mut Context<Self>) -> bool {
        let removed = self.review.remove_comment(id).is_some();
        if removed {
            cx.notify();
        }
        removed
    }

    pub(crate) const fn options(&self) -> &DiffViewerOptions {
        &self.options
    }

    pub(crate) fn select_file(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.document.files.len() {
            self.selected_file = index;
            self.draft = None;
            self.select_first_row();
            cx.notify();
        }
    }

    /// Replaces the active draft's body, returning whether a draft exists.
    pub fn set_draft_body(&mut self, body: impl Into<String>, cx: &mut Context<Self>) -> bool {
        let Some(draft) = &mut self.draft else {
            return false;
        };
        draft.body = body.into();
        cx.notify();
        true
    }

    /// Returns the active draft body, if a line is being commented on.
    #[must_use]
    pub fn draft_body(&self) -> Option<&str> {
        self.draft.as_ref().map(|draft| draft.body.as_str())
    }

    pub(crate) fn begin_comment(
        &mut self,
        anchor: LineAnchor,
        line_text: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.draft = Some(CommentDraft::new(anchor, line_text));
        cx.notify();
    }

    pub(crate) fn highlight_cell(
        &mut self,
        row_index: usize,
        side: diff_core::DiffSide,
        source: &str,
    ) -> Vec<HighlightSpan> {
        let language = self
            .presentation
            .row(row_index)
            .and_then(|row| self.document.files.get(row.file_index))
            .map_or("", |file| file.language());
        let _ = side;
        self.highlighter.highlight(&self.theme, language, source)
    }

    pub(crate) fn record_visible_range(&mut self, range: Range<usize>) {
        self.rendered_visible_rows = range.len();
        self.last_visible_range = range;
    }

    fn rebuild_presentation(&mut self) {
        self.presentation = build_presentation(
            self.document.clone(),
            self.requested_mode,
            self.split_when_auto,
        );
    }

    fn select_first_row(&mut self) {
        self.selected_row = self
            .presentation
            .file_range(self.selected_file)
            .and_then(|range| range.into_iter().find(|index| self.row_has_anchor(*index)))
            .unwrap_or(0);
    }

    fn row_has_anchor(&self, index: usize) -> bool {
        self.presentation.row(index).is_some_and(|row| {
            row.left
                .as_ref()
                .and_then(|cell| cell.anchor.as_ref())
                .is_some()
                || row
                    .right
                    .as_ref()
                    .and_then(|cell| cell.anchor.as_ref())
                    .is_some()
        })
    }

    fn move_file(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.document.files.is_empty() {
            return;
        }
        self.selected_file = offset(self.selected_file, delta, self.document.files.len() - 1);
        self.select_first_row();
        self.draft = None;
        cx.notify();
    }

    fn move_hunk(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(file) = self.document.files.get(self.selected_file) else {
            return;
        };
        if file.hunks.is_empty() {
            return;
        }
        let current = self
            .presentation
            .row(self.selected_row)
            .and_then(|row| row.hunk_index)
            .unwrap_or(0);
        let target = offset(current, delta, file.hunks.len() - 1);
        let Some(range) = self.presentation.hunk_range(self.selected_file, target) else {
            return;
        };
        self.selected_row = range
            .into_iter()
            .find(|index| self.row_has_anchor(*index))
            .unwrap_or(self.selected_row);
        cx.notify();
    }

    fn selected_target(&self) -> Option<(LineAnchor, String)> {
        let row = self.presentation.row(self.selected_row)?;
        let cell = row.right.as_ref().or(row.left.as_ref())?;
        Some((cell.anchor.clone()?, cell.text.to_string()))
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
        let mode = match self.requested_mode {
            ViewMode::Auto => ViewMode::Unified,
            ViewMode::Unified => ViewMode::Split,
            ViewMode::Split => ViewMode::Auto,
        };
        self.set_view_mode(mode, cx);
    }

    fn add_comment_action(&mut self, _: &AddComment, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((anchor, text)) = self.selected_target() {
            self.begin_comment(anchor, text, window, cx);
        }
    }

    fn edit_comment_action(&mut self, _: &EditComment, _: &mut Window, cx: &mut Context<Self>) {
        let Some((anchor, line_text)) = self.selected_target() else {
            return;
        };
        let Some(comment) = self
            .review
            .comments()
            .iter()
            .rev()
            .find(|comment| comment.anchor == anchor)
        else {
            return;
        };
        self.draft = Some(CommentDraft::editing(
            anchor,
            line_text,
            comment.body.clone(),
            comment.id,
        ));
        cx.notify();
    }

    fn delete_comment_action(&mut self, _: &DeleteComment, _: &mut Window, cx: &mut Context<Self>) {
        let Some((anchor, _)) = self.selected_target() else {
            return;
        };
        if let Some(id) = self
            .review
            .comments()
            .iter()
            .rev()
            .find(|comment| comment.anchor == anchor)
            .map(|comment| comment.id)
        {
            self.remove_comment(id, cx);
        }
    }

    fn on_draft_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = &mut self.draft else {
            return;
        };
        if event.keystroke.key == "backspace" {
            draft.backspace();
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
            draft.push(text);
            cx.notify();
        }
    }

    fn submit_comment(&mut self, _: &SubmitComment, _: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.draft.take() else {
            return;
        };
        if draft.body.trim().is_empty() {
            cx.notify();
            return;
        }
        if let Some(id) = draft.editing {
            self.review.edit_comment(id, draft.body);
        } else {
            self.review
                .add_comment_with_context(draft.anchor, draft.line_text, draft.body);
        }
        cx.notify();
    }

    fn cancel_comment(&mut self, _: &CancelComment, _: &mut Window, cx: &mut Context<Self>) {
        self.draft = None;
        cx.notify();
    }

    pub(crate) fn copy_review(&mut self, _: &CopyReview, _: &mut Window, cx: &mut Context<Self>) {
        if !self.review.is_empty() {
            cx.emit(DiffViewerEvent::CopyFormattedReview(
                self.review.submission().formatted,
            ));
        }
    }

    pub(crate) fn submit_review(
        &mut self,
        _: &SubmitReview,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.emit(DiffViewerEvent::SubmitReview(self.review.submission()));
    }

    pub(crate) fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        let _ = self;
        cx.emit(DiffViewerEvent::Cancel);
    }

    fn update_auto_mode(&mut self, width: f32) {
        let split = width >= self.options.auto_split_width;
        if self.requested_mode == ViewMode::Auto && split != self.split_when_auto {
            self.split_when_auto = split;
            self.rebuild_presentation();
            self.select_first_row();
        }
    }
}

impl EventEmitter<DiffViewerEvent> for DiffViewer {}

impl Render for DiffViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width = f32::from(window.viewport_size().width) - self.options.sidebar_width;
        self.update_auto_mode(width);
        let palette = self.theme.palette.clone();
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
            .when_some(self.draft.as_ref(), |root, draft| {
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
                        .child(format!(
                            "Comment on {} · type, then press ⌘/Ctrl+Enter: {}▏",
                            draft.anchor.path, draft.body
                        )),
                )
            })
            .child(self.render_review_bar(cx))
    }
}

fn build_presentation(
    document: Arc<DiffDocument>,
    view_mode: ViewMode,
    split_when_auto: bool,
) -> DiffPresentation {
    DiffPresentation::new(
        document,
        PresentationOptions {
            view_mode,
            split_when_auto,
            include_file_headers: true,
        },
    )
}

fn offset(value: usize, delta: isize, maximum: usize) -> usize {
    if delta.is_negative() {
        value.saturating_sub(delta.unsigned_abs())
    } else {
        value.saturating_add(delta.unsigned_abs()).min(maximum)
    }
}
