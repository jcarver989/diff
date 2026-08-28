use diff_core::{
    DiffDocument, DiffPresentation, DiffSide, DiffTheme, HighlightStats, Layout, Review,
    ReviewSession, SyntaxHighlighter, ViewMode,
};
use ratatui::layout::{Position, Rect};
use std::sync::Arc;

pub(crate) const SPLIT_BREAKPOINT: u16 = 96;

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

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct HitLayout {
    pub drawer: Rect,
    pub patch: Rect,
}

/// Persistent state for [`crate::DiffReviewWidget`].
#[derive(Debug)]
pub struct DiffReviewState {
    pub(crate) session: ReviewSession,
    pub(crate) theme: DiffTheme,
    pub(crate) highlighter: SyntaxHighlighter,
    pub(crate) status: DiffReviewStatus,
    pub(crate) focus: FocusPane,
    pub(crate) drawer_scroll: usize,
    pub(crate) drawer_height: usize,
    pub(crate) scroll: usize,
    pub(crate) last_height: usize,
    pub(crate) presentation_width: u16,
    pub(crate) help: bool,
    pub(crate) hit_layout: HitLayout,
    pub(crate) visible_rows: Vec<(u16, usize)>,
    pub(crate) cursor_position: Option<Position>,
    pub(crate) follow_pending: bool,
    pub(crate) dirty: bool,
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
        let mut state = Self {
            session: ReviewSession::new(document),
            theme,
            highlighter: SyntaxHighlighter::default(),
            status: DiffReviewStatus::Ready,
            focus: FocusPane::Files,
            drawer_scroll: 0,
            drawer_height: 1,
            scroll: 0,
            last_height: 0,
            presentation_width: 0,
            help: false,
            hit_layout: HitLayout::default(),
            visible_rows: Vec::new(),
            cursor_position: None,
            follow_pending: true,
            dirty: true,
        };
        state.scroll_to_selected_file();
        state
    }

    /// Creates loading state with an empty placeholder document.
    #[must_use]
    pub fn loading() -> Self {
        let mut state = Self::new(Arc::new(DiffDocument::empty()));
        state.status = DiffReviewStatus::Loading;
        state
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

    /// Returns the structured review.
    #[must_use]
    pub const fn review(&self) -> &Review {
        self.session.review()
    }

    /// Returns mutable review access for host-driven operations.
    pub const fn review_mut(&mut self) -> &mut Review {
        self.session.review_mut()
    }

    #[must_use]
    pub const fn presentation(&self) -> &DiffPresentation {
        self.session.presentation()
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
        self.session.selected_file()
    }

    /// Returns the selected presentation-row index, when rows exist.
    #[must_use]
    pub fn selected_row(&self) -> Option<usize> {
        self.session.selected_row()
    }

    #[must_use]
    pub const fn selected_side(&self) -> DiffSide {
        self.session.selected_side()
    }

    /// Returns syntax cache and work counters.
    #[must_use]
    pub const fn highlight_stats(&self) -> HighlightStats {
        self.highlighter.stats()
    }

    /// Returns the first visible presentation-row index.
    #[must_use]
    pub const fn scroll_offset(&self) -> usize {
        self.scroll
    }

    /// Returns whether anything since the last frame changed what a redraw
    /// would show.
    ///
    /// Hosts may skip drawing [`crate::DiffReviewWidget`] while this is false,
    /// which keeps pointer motion in a terminal reporting all mouse movement
    /// from costing a frame each. Rendering the widget clears it.
    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Marks the state as needing a redraw, for changes the widget cannot
    /// observe on its own, such as a terminal resize.
    pub const fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Returns the requested view mode.
    #[must_use]
    pub const fn view_mode(&self) -> ViewMode {
        self.session.view_mode()
    }

    #[must_use]
    pub const fn layout(&self) -> Layout {
        self.session.layout()
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
        self.session.set_document(document);
        self.status = DiffReviewStatus::Ready;
        self.cursor_position = None;
        self.mark_dirty();
        self.drawer_scroll = self
            .drawer_scroll
            .min(self.session.selected_file().unwrap_or(0));
        self.scroll_to_selected_file();
    }

    /// Marks the state as waiting for a host snapshot.
    pub fn set_loading(&mut self) {
        self.status = DiffReviewStatus::Loading;
        self.session.cancel_draft();
        self.cursor_position = None;
        self.mark_dirty();
    }

    /// Shows a host-provided loading error.
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.status = DiffReviewStatus::Error(message.into());
        self.session.cancel_draft();
        self.cursor_position = None;
        self.mark_dirty();
    }

    /// Changes the neutral theme and clears cached syntax spans.
    pub fn set_theme(&mut self, theme: DiffTheme) {
        self.theme = theme;
        self.highlighter.clear_cache();
        self.mark_dirty();
    }

    /// Selects automatic, unified, or split presentation.
    pub fn set_view_mode(&mut self, mode: ViewMode) {
        self.mark_dirty();
        if self.session.set_view_mode(mode) {
            self.scroll_to_selected_file();
        }
    }

    /// Clears all queued comments and any active draft.
    pub fn clear_review(&mut self) {
        self.session.clear_review();
        self.cursor_position = None;
        self.mark_dirty();
    }

    pub(crate) fn ensure_presentation(&mut self, width: u16) {
        self.presentation_width = width;
        if self.session.set_split_when_auto(width >= SPLIT_BREAKPOINT) {
            self.scroll_to_selected_file();
        }
    }

    pub(crate) fn scroll_to_selected_file(&mut self) {
        self.scroll = self
            .session
            .selected_file_range()
            .map_or(0, |range| range.start);
        self.request_follow();
    }

    pub(crate) fn move_file(&mut self, delta: isize) {
        let Some(selected) = self.session.move_file(delta) else {
            return;
        };
        self.follow_file_selection(selected);
    }

    pub(crate) fn select_file(&mut self, index: usize) {
        if self.session.select_file(index) {
            self.follow_file_selection(index);
        }
    }

    fn follow_file_selection(&mut self, selected: usize) {
        if selected < self.drawer_scroll {
            self.drawer_scroll = selected;
        } else if selected >= self.drawer_scroll.saturating_add(self.drawer_height) {
            self.drawer_scroll = selected.saturating_sub(self.drawer_height.saturating_sub(1));
        }
        self.scroll_to_selected_file();
    }

    pub(crate) fn move_row(&mut self, delta: isize) {
        self.session.move_row(delta);
        self.request_follow();
    }

    pub(crate) fn select_boundary(&mut self, end: bool) {
        self.session.select_boundary(end);
        self.request_follow();
    }

    /// Scrolls the patch viewport without moving the selection, the way a
    /// mouse wheel does. The selection may leave the viewport; the next
    /// keyboard move brings it back.
    pub(crate) fn scroll_patch(&mut self, delta: isize) {
        let Some(range) = self.session.selected_file_range() else {
            return;
        };
        if range.is_empty() {
            return;
        }
        let target = if delta.is_negative() {
            self.scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.scroll.saturating_add(delta.unsigned_abs())
        };
        let clamped = target.clamp(range.start, range.end - 1);
        if clamped != self.scroll {
            self.scroll = clamped;
            self.mark_dirty();
        }
    }

    /// Scrolls the file drawer without moving the selected file.
    pub(crate) fn scroll_drawer(&mut self, delta: isize) {
        let last = self
            .document()
            .files
            .len()
            .saturating_sub(self.drawer_height);
        let target = if delta.is_negative() {
            self.drawer_scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.drawer_scroll.saturating_add(delta.unsigned_abs())
        };
        let clamped = target.min(last);
        if clamped != self.drawer_scroll {
            self.drawer_scroll = clamped;
            self.mark_dirty();
        }
    }

    pub(crate) fn page(&mut self, delta: isize) {
        let height = isize::try_from(self.last_height.max(1)).unwrap_or(isize::MAX);
        self.move_row(delta.saturating_mul(height));
    }

    /// Brings the selection back into view against the height the last frame
    /// measured, and asks the next frame to redo it once it knows its own.
    /// Before any frame has drawn there is no height to work from, so the
    /// request only carries over.
    pub(crate) fn request_follow(&mut self) {
        self.follow_pending = true;
        self.mark_dirty();
        self.follow_selection();
    }

    pub(crate) fn take_follow_request(&mut self) -> bool {
        std::mem::take(&mut self.follow_pending)
    }

    pub(crate) fn follow_selection(&mut self) {
        if self.last_height == 0 {
            return;
        }
        let Some(selected) = self.session.selected_row() else {
            return;
        };
        if selected < self.scroll {
            self.scroll = selected;
            return;
        }

        let height = self.last_height.max(1);
        let range_start = self
            .session
            .selected_file_range()
            .map_or(0, |range| range.start);
        let mut earliest = selected;
        let mut used = 1_usize;
        while earliest > range_start {
            let previous = earliest - 1;
            let cost = self.rendered_height(previous);
            if used.saturating_add(cost) > height {
                break;
            }
            used = used.saturating_add(cost);
            earliest = previous;
        }
        if self.scroll < earliest {
            self.scroll = earliest;
        }
    }

    fn rendered_height(&self, index: usize) -> usize {
        let Some(row) = self.session.presentation().row(index) else {
            return 0;
        };
        let comments = self
            .session
            .review()
            .comments()
            .iter()
            .filter(|comment| {
                self.session
                    .presentation()
                    .row_shows_anchor(row, &comment.anchor)
            })
            .count();
        let draft = usize::from(self.session.draft().is_some_and(|draft| {
            self.session
                .presentation()
                .row_shows_anchor(row, draft.anchor())
        }));
        1 + comments + draft
    }

    pub(crate) fn select_clicked_row(&mut self, row: u16) {
        let clicked = self
            .visible_rows
            .iter()
            .find(|(screen_row, _)| *screen_row == row)
            .map(|(_, index)| *index);
        if let Some(index) = clicked
            && self.session.select_row(index)
        {
            self.request_follow();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diff_core::FileDiff;

    #[test]
    fn comments_above_selection_count_toward_viewport_height() {
        let document = Arc::new(DiffDocument {
            repo_root: "/repo".into(),
            files: vec![FileDiff::from_texts("a.rs", "a\nb\nc\n", "A\nB\nC\n").unwrap()],
        });
        let mut state = DiffReviewState::new(document);
        state.last_height = 3;
        let anchor = state.session.selected_anchor().unwrap();
        state.session.review_mut().add_comment(anchor, "note");
        state.session.move_row(2);
        state.follow_selection();
        assert!(state.scroll > state.session.selected_file_range().unwrap().start);
    }
}
