//! Stateful document, selection, viewport, and review behavior.

use diff_core::{
    DiffDocument, DiffPresentation, DiffTheme, HighlightStats, LineAnchor, PresentationOptions,
    PresentedCell, Review, RowKind, SyntaxHighlighter, ViewMode,
};
use ratatui::layout::{Position, Rect};
use std::sync::Arc;

/// Which pane receives navigation input.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FocusPane {
    /// File navigation drawer.
    #[default]
    Files,
    /// Diff document.
    Diff,
}

/// Current host-provided document state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffReviewStatus {
    /// A host is loading a snapshot.
    Loading,
    /// A document is ready for review.
    Ready,
    /// Loading failed with a host-provided message.
    Error(String),
}

#[derive(Debug, Clone)]
pub(crate) struct Draft {
    pub anchor: LineAnchor,
    pub line_text: String,
    pub text: String,
    pub cursor: usize,
    pub editing: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct HitLayout {
    pub drawer: Rect,
    pub patch: Rect,
}

/// Persistent state for [`crate::DiffReviewWidget`].
#[derive(Debug)]
pub struct DiffReviewState {
    pub(crate) document: Arc<DiffDocument>,
    pub(crate) presentation: DiffPresentation,
    pub(crate) presentation_width: u16,
    pub(crate) theme: DiffTheme,
    pub(crate) highlighter: SyntaxHighlighter,
    pub(crate) status: DiffReviewStatus,
    pub(crate) focus: FocusPane,
    pub(crate) selected_file: usize,
    pub(crate) drawer_scroll: usize,
    pub(crate) drawer_height: usize,
    pub(crate) selected_row: usize,
    pub(crate) scroll: usize,
    pub(crate) view_mode: ViewMode,
    pub(crate) review: Review,
    pub(crate) draft: Option<Draft>,
    pub(crate) help: bool,
    pub(crate) hit_layout: HitLayout,
    pub(crate) visible_rows: Vec<(u16, usize)>,
    pub(crate) last_height: usize,
    pub(crate) cursor_position: Option<Position>,
}

impl DiffReviewState {
    /// Creates ready state from an immutable document snapshot.
    #[must_use]
    pub fn new(document: Arc<DiffDocument>) -> Self {
        Self::with_theme(document, DiffTheme::default())
    }

    /// Creates ready state with a shared neutral theme.
    #[must_use]
    pub fn with_theme(document: Arc<DiffDocument>, theme: DiffTheme) -> Self {
        let presentation = DiffPresentation::new(document.clone(), PresentationOptions::default());
        let mut state = Self {
            document,
            presentation,
            presentation_width: 0,
            theme,
            highlighter: SyntaxHighlighter::default(),
            status: DiffReviewStatus::Ready,
            focus: FocusPane::Files,
            selected_file: 0,
            drawer_scroll: 0,
            drawer_height: 1,
            selected_row: 0,
            scroll: 0,
            view_mode: ViewMode::Auto,
            review: Review::default(),
            draft: None,
            help: false,
            hit_layout: HitLayout::default(),
            visible_rows: Vec::new(),
            last_height: 1,
            cursor_position: None,
        };
        state.select_file_row();
        state
    }

    /// Creates loading state with an empty placeholder document.
    #[must_use]
    pub fn loading() -> Self {
        let mut state = Self::new(Arc::new(DiffDocument {
            repo_root: String::new(),
            files: Vec::new(),
        }));
        state.status = DiffReviewStatus::Loading;
        state
    }

    /// Returns the current immutable snapshot.
    #[must_use]
    pub fn document(&self) -> &Arc<DiffDocument> {
        &self.document
    }

    /// Returns the structured review.
    #[must_use]
    pub const fn review(&self) -> &Review {
        &self.review
    }

    /// Returns mutable review access for host-driven operations.
    pub const fn review_mut(&mut self) -> &mut Review {
        &mut self.review
    }

    /// Returns the current load status.
    #[must_use]
    pub const fn status(&self) -> &DiffReviewStatus {
        &self.status
    }

    /// Returns the focused pane.
    #[must_use]
    pub const fn focus(&self) -> FocusPane {
        self.focus
    }

    /// Returns the selected file index, when the document has files.
    #[must_use]
    pub fn selected_file(&self) -> Option<usize> {
        (!self.document.files.is_empty()).then_some(self.selected_file)
    }

    /// Returns the selected presentation-row index, when rows exist.
    #[must_use]
    pub fn selected_row(&self) -> Option<usize> {
        (self.presentation.row_count() != 0).then_some(self.selected_row)
    }

    /// Returns the currently indexed presentation.
    #[must_use]
    pub const fn presentation(&self) -> &DiffPresentation {
        &self.presentation
    }

    /// Returns syntax cache and work counters.
    #[must_use]
    pub fn highlight_stats(&self) -> HighlightStats {
        self.highlighter.stats()
    }

    /// Returns the first visible presentation-row index.
    #[must_use]
    pub const fn scroll_offset(&self) -> usize {
        self.scroll
    }

    /// Returns the requested view mode.
    #[must_use]
    pub const fn view_mode(&self) -> ViewMode {
        self.view_mode
    }

    /// Returns the terminal cursor position for an active comment draft.
    ///
    /// Hosts may pass this to `Frame::set_cursor_position` after rendering.
    #[must_use]
    pub const fn cursor_position(&self) -> Option<Position> {
        self.cursor_position
    }

    /// Replaces the snapshot while retaining and reconciling review comments.
    pub fn set_document(&mut self, document: Arc<DiffDocument>) {
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
        self.drawer_scroll = self.drawer_scroll.min(self.selected_file);
        self.status = DiffReviewStatus::Ready;
        self.draft = None;
        self.cursor_position = None;
        self.rebuild_presentation(self.presentation_width);
        self.select_file_row();
    }

    /// Marks the state as waiting for a host snapshot.
    pub fn set_loading(&mut self) {
        self.status = DiffReviewStatus::Loading;
        self.draft = None;
        self.cursor_position = None;
    }

    /// Shows a host-provided loading error.
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.status = DiffReviewStatus::Error(message.into());
        self.draft = None;
        self.cursor_position = None;
    }

    /// Changes the neutral theme and clears cached syntax spans.
    pub fn set_theme(&mut self, theme: DiffTheme) {
        self.theme = theme;
        self.highlighter.clear_cache();
    }

    /// Selects automatic, unified, or split presentation.
    pub fn set_view_mode(&mut self, mode: ViewMode) {
        if self.view_mode != mode {
            self.view_mode = mode;
            self.rebuild_presentation(self.presentation_width);
            self.select_file_row();
        }
    }

    /// Clears all queued comments and any active draft.
    pub fn clear_review(&mut self) {
        self.review.clear();
        self.draft = None;
        self.cursor_position = None;
    }

    pub(crate) fn ensure_presentation(&mut self, width: u16) {
        let auto_changed =
            self.view_mode == ViewMode::Auto && (self.presentation_width >= 96) != (width >= 96);
        if self.presentation_width != width && auto_changed {
            self.rebuild_presentation(width);
            self.select_file_row();
        } else {
            self.presentation_width = width;
        }
    }

    pub(crate) fn rebuild_presentation(&mut self, width: u16) {
        self.presentation_width = width;
        self.presentation = DiffPresentation::new(
            self.document.clone(),
            PresentationOptions {
                view_mode: self.view_mode,
                split_when_auto: width >= 96,
                include_file_headers: true,
            },
        );
        self.selected_row = self
            .selected_row
            .min(self.presentation.row_count().saturating_sub(1));
        self.scroll = self.scroll.min(self.selected_row);
    }

    pub(crate) fn select_file_row(&mut self) {
        if let Some(range) = self.presentation.file_range(self.selected_file) {
            self.scroll = range.start;
            self.selected_row = range
                .clone()
                .find(|index| self.row_is_selectable(*index))
                .unwrap_or(range.start);
        } else {
            self.selected_row = 0;
            self.scroll = 0;
        }
    }

    pub(crate) fn move_file(&mut self, delta: isize) {
        if self.document.files.is_empty() {
            return;
        }
        self.selected_file = offset(self.selected_file, delta, self.document.files.len() - 1);
        if self.selected_file < self.drawer_scroll {
            self.drawer_scroll = self.selected_file;
        } else if self.selected_file >= self.drawer_scroll.saturating_add(self.drawer_height) {
            self.drawer_scroll = self
                .selected_file
                .saturating_sub(self.drawer_height.saturating_sub(1));
        }
        self.select_file_row();
    }

    pub(crate) fn move_row(&mut self, delta: isize) {
        let Some(range) = self.presentation.file_range(self.selected_file) else {
            return;
        };
        let selectable: Vec<usize> = range
            .filter(|index| self.row_is_selectable(*index))
            .collect();
        if selectable.is_empty() {
            return;
        }
        let position = selectable
            .iter()
            .position(|index| *index == self.selected_row)
            .unwrap_or(0);
        self.selected_row = selectable[offset(position, delta, selectable.len() - 1)];
        self.follow_selection();
    }

    pub(crate) fn select_boundary(&mut self, end: bool) {
        let Some(mut range) = self.presentation.file_range(self.selected_file) else {
            return;
        };
        let selected = if end {
            range.rev().find(|index| self.row_is_selectable(*index))
        } else {
            range.find(|index| self.row_is_selectable(*index))
        };
        if let Some(index) = selected {
            self.selected_row = index;
            self.follow_selection();
        }
    }

    pub(crate) fn page(&mut self, delta: isize) {
        let height = isize::try_from(self.last_height.max(1)).unwrap_or(isize::MAX);
        self.move_row(delta.saturating_mul(height));
    }

    pub(crate) fn follow_selection(&mut self) {
        if self.selected_row < self.scroll {
            self.scroll = self.selected_row;
        } else if self.selected_row >= self.scroll.saturating_add(self.last_height) {
            self.scroll = self
                .selected_row
                .saturating_sub(self.last_height.saturating_sub(1));
        }
    }

    pub(crate) fn selected_cell(&self) -> Option<&PresentedCell> {
        let row = self.presentation.row(self.selected_row)?;
        row.right.as_ref().or(row.left.as_ref())
    }

    pub(crate) fn begin_draft(&mut self, editing: Option<u64>) {
        let Some(cell) = self.selected_cell() else {
            return;
        };
        let Some(anchor) = cell.anchor.clone() else {
            return;
        };
        let text = editing
            .and_then(|id| {
                self.review
                    .comments()
                    .iter()
                    .find(|comment| comment.id == id)
            })
            .map_or_else(String::new, |comment| comment.body.clone());
        let cursor = text.len();
        self.draft = Some(Draft {
            anchor,
            line_text: cell.text.to_string(),
            text,
            cursor,
            editing,
        });
    }

    pub(crate) fn submit_draft(&mut self) {
        let Some(draft) = self.draft.take() else {
            return;
        };
        if draft.text.trim().is_empty() {
            return;
        }
        if let Some(id) = draft.editing {
            self.review.edit_comment(id, draft.text);
        } else {
            self.review
                .add_comment_with_context(draft.anchor, draft.line_text, draft.text);
        }
    }

    pub(crate) fn last_comment_for_selection(&self) -> Option<u64> {
        let anchor = self.selected_cell()?.anchor.as_ref()?;
        self.review
            .comments()
            .iter()
            .rev()
            .find(|comment| &comment.anchor == anchor)
            .map(|comment| comment.id)
    }

    pub(crate) fn row_is_selectable(&self, index: usize) -> bool {
        self.presentation.row(index).is_some_and(|row| {
            row.kind == RowKind::Code
                && [row.left.as_ref(), row.right.as_ref()]
                    .into_iter()
                    .flatten()
                    .any(|cell| cell.anchor.is_some())
        })
    }

    pub(crate) fn select_clicked_row(&mut self, row: u16) {
        if let Some((_, index)) = self
            .visible_rows
            .iter()
            .find(|(screen_row, index)| *screen_row == row && self.row_is_selectable(*index))
        {
            self.selected_row = *index;
            self.follow_selection();
        }
    }
}

fn offset(value: usize, delta: isize, maximum: usize) -> usize {
    if delta.is_negative() {
        value.saturating_sub(delta.unsigned_abs())
    } else {
        value.saturating_add(delta.unsigned_abs()).min(maximum)
    }
}
