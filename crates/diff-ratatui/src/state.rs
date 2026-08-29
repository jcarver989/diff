use crate::{
    drawer::{DrawerEntry, DrawerTree},
    patch_layout::PatchVisualLayout,
};
use diff_core::{
    DiffDocument, DiffPresentation, DiffSide, DiffTheme, FileStatus, HighlightStats, Layout,
    RepositoryAction, Review, ReviewSession, StageState, SyntaxHighlighter, ViewMode,
};
use ratatui::layout::{Position, Rect};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

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
    pub drawer_stage_column: Option<u16>,
    pub patch: Rect,
}

#[derive(Debug, Clone, Copy, Default)]
enum DrawerFollow {
    #[default]
    Pending,
    Settled,
}

#[derive(Debug)]
pub(crate) enum RepositoryPrompt {
    Commit {
        message: String,
    },
    Discard {
        path: diff_core::RepoPath,
        status: FileStatus,
    },
}

/// Status of the most recent repository mutation requested by the UI.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RepositoryOperationStatus {
    #[default]
    Idle,
    Pending,
    Error(String),
}

#[derive(Debug)]
struct CachedPatchLayout {
    key: u64,
    layout: Arc<PatchVisualLayout>,
}

/// Persistent state for [`crate::DiffReviewWidget`].
#[derive(Debug)]
pub struct DiffReviewState {
    pub(crate) session: ReviewSession,
    pub(crate) theme: DiffTheme,
    pub(crate) highlighter: SyntaxHighlighter,
    pub(crate) status: DiffReviewStatus,
    pub(crate) focus: FocusPane,
    pub(crate) drawer: DrawerTree,
    pub(crate) drawer_selected: usize,
    pub(crate) drawer_scroll: usize,
    pub(crate) drawer_height: usize,
    drawer_follow: DrawerFollow,
    pub(crate) scroll: usize,
    pub(crate) last_height: usize,
    pub(crate) presentation_width: u16,
    patch_layout: Option<CachedPatchLayout>,
    pub(crate) help: bool,
    pub(crate) repository_prompt: Option<RepositoryPrompt>,
    pub(crate) repository_status: RepositoryOperationStatus,
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
        let drawer = DrawerTree::new(&document);
        let drawer_selected = drawer.position_of_file(0).unwrap_or(0);
        let mut state = Self {
            session: ReviewSession::new(document),
            theme,
            highlighter: SyntaxHighlighter::default(),
            status: DiffReviewStatus::Ready,
            focus: FocusPane::Files,
            drawer,
            drawer_selected,
            drawer_scroll: 0,
            drawer_height: 1,
            drawer_follow: DrawerFollow::Pending,
            scroll: 0,
            last_height: 0,
            presentation_width: 0,
            patch_layout: None,
            help: false,
            repository_prompt: None,
            repository_status: RepositoryOperationStatus::Idle,
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

    /// Returns the first visible rendered-row index in the selected file.
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
        self.repository_status = RepositoryOperationStatus::Idle;
        self.cursor_position = None;
        self.mark_dirty();
        let document = self.document().clone();
        self.drawer.rebuild(&document);
        if let Some(selected) = self.session.selected_file() {
            self.drawer.expand_file(&document, selected);
            self.drawer_selected = self.drawer.position_of_file(selected).unwrap_or(0);
        } else {
            self.drawer_selected = 0;
        }
        self.follow_drawer_selection();
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

    /// Marks a repository mutation as in flight while retaining the current snapshot.
    pub fn set_repository_pending(&mut self) {
        self.repository_status = RepositoryOperationStatus::Pending;
        self.repository_prompt = None;
        self.mark_dirty();
    }

    /// Shows a repository mutation error while retaining the current snapshot.
    pub fn set_repository_error(&mut self, message: impl Into<String>) {
        self.repository_status = RepositoryOperationStatus::Error(message.into());
        self.repository_prompt = None;
        self.mark_dirty();
    }

    pub(crate) fn toggle_stage_action(&self) -> Option<RepositoryAction> {
        let entry = self.drawer.entry(self.drawer_selected)?;
        let document = self.document();
        let state = DrawerTree::stage_state_for_entry(document, entry);
        let paths = DrawerTree::paths_for_entry(document, entry);
        if paths.is_empty() {
            return None;
        }
        Some(if state == StageState::Staged {
            RepositoryAction::UnstagePaths(paths)
        } else {
            RepositoryAction::StagePaths(paths)
        })
    }

    pub(crate) fn begin_commit(&mut self) {
        self.repository_prompt = Some(RepositoryPrompt::Commit {
            message: String::new(),
        });
    }

    pub(crate) fn begin_discard(&mut self) {
        let Some(file) = self
            .selected_file()
            .and_then(|index| self.document().files.get(index))
        else {
            return;
        };
        self.repository_prompt = Some(RepositoryPrompt::Discard {
            path: file.path.clone(),
            status: file.status,
        });
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
        let width_changed = self.presentation_width != width;
        self.presentation_width = width;
        if self.session.set_split_when_auto(width >= SPLIT_BREAKPOINT) {
            self.scroll_to_selected_file();
        } else if width_changed {
            self.request_follow();
        }
    }

    pub(crate) fn scroll_to_selected_file(&mut self) {
        self.scroll = 0;
        self.request_follow();
    }

    pub(crate) fn move_drawer_entry(&mut self, delta: isize) {
        let last = self.drawer.entries().len().saturating_sub(1);
        self.drawer_selected = offset(self.drawer_selected, delta, last);
        if let Some(DrawerEntry::File { index, .. }) = self.drawer.entry(self.drawer_selected) {
            let index = *index;
            if self.session.select_file(index) {
                self.scroll_to_selected_file();
            }
        }
        self.follow_drawer_selection();
    }

    pub(crate) fn select_drawer_entry(&mut self, index: usize) {
        if index >= self.drawer.entries().len() {
            return;
        }
        self.drawer_selected = index;
        if let Some(DrawerEntry::File { index, .. }) = self.drawer.entry(index) {
            let index = *index;
            if self.session.select_file(index) {
                self.scroll_to_selected_file();
            }
        }
        self.follow_drawer_selection();
    }

    /// Expands the selected directory, or selects the current file. Returns
    /// whether the selected entry was a directory.
    pub(crate) fn expand_or_open_drawer_entry(&mut self) -> bool {
        match self.drawer.entry(self.drawer_selected).cloned() {
            Some(DrawerEntry::Directory { path, .. }) => {
                self.drawer.expand(&path);
                self.drawer_selected = self.drawer.position_of_directory(&path).unwrap_or(0);
                self.follow_drawer_selection();
                self.mark_dirty();
                true
            }
            Some(DrawerEntry::File { index, .. }) => {
                if self.session.select_file(index) {
                    self.scroll_to_selected_file();
                }
                false
            }
            None => false,
        }
    }

    pub(crate) fn collapse_drawer_entry(&mut self) {
        let Some(DrawerEntry::Directory { path, .. }) =
            self.drawer.entry(self.drawer_selected).cloned()
        else {
            return;
        };
        self.drawer.collapse(&path);
        self.drawer_selected = self.drawer.position_of_directory(&path).unwrap_or(0);
        self.follow_drawer_selection();
        self.mark_dirty();
    }

    fn follow_drawer_selection(&mut self) {
        self.drawer_follow = DrawerFollow::Pending;
        if self.drawer_selected < self.drawer_scroll {
            self.drawer_scroll = self.drawer_selected;
        } else if self.drawer_selected >= self.drawer_scroll.saturating_add(self.drawer_height) {
            self.drawer_scroll = self
                .drawer_selected
                .saturating_sub(self.drawer_height.saturating_sub(1));
        }
    }

    pub(crate) fn take_drawer_follow_request(&mut self) -> bool {
        matches!(
            std::mem::replace(&mut self.drawer_follow, DrawerFollow::Settled),
            DrawerFollow::Pending
        )
    }

    pub(crate) fn move_row(&mut self, delta: isize) {
        let selected = self.session.selected_row();
        let side = self.session.selected_side();
        self.session.move_row(delta);
        if self.session.selected_row() != selected || self.session.selected_side() != side {
            self.request_follow();
        }
    }

    pub(crate) fn select_boundary(&mut self, end: bool) {
        self.session.select_boundary(end);
        self.request_follow();
    }

    /// Scrolls the patch viewport without moving the selection. The selection
    /// may leave the viewport; the next selection move brings it back.
    pub(crate) fn scroll_patch(&mut self, delta: isize) {
        let Some(layout) = self.patch_visual_layout() else {
            return;
        };
        if layout.is_empty() {
            return;
        }
        let target = if delta.is_negative() {
            self.scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.scroll.saturating_add(delta.unsigned_abs())
        };
        let last = layout.len().saturating_sub(self.last_height.max(1));
        let clamped = target.min(last);
        if clamped != self.scroll {
            self.scroll = clamped;
            self.mark_dirty();
        }
    }

    /// Scrolls the file drawer without moving the selected file.
    pub(crate) fn scroll_drawer(&mut self, delta: isize) {
        let last = self
            .drawer
            .entries()
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
        self.scroll_patch(delta.saturating_mul(height));
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
        let draft = self.session.draft().is_some();
        let Some(layout) = self.patch_visual_layout() else {
            return;
        };
        let Some(target) = layout.focused_visual_row(selected, draft) else {
            return;
        };
        let height = self.last_height.max(1);
        if target < self.scroll {
            self.scroll = target;
        } else if target >= self.scroll.saturating_add(height) {
            self.scroll = target.saturating_sub(height.saturating_sub(1));
        }
        self.scroll = self.scroll.min(layout.len().saturating_sub(height));
    }

    pub(crate) fn patch_visual_layout(&mut self) -> Option<Arc<PatchVisualLayout>> {
        let range = self.session.selected_file_range()?;
        let key = self.patch_layout_key(&range);
        let rebuild = self
            .patch_layout
            .as_ref()
            .is_none_or(|cached| cached.key != key);
        if rebuild {
            self.patch_layout = Some(CachedPatchLayout {
                key,
                layout: Arc::new(PatchVisualLayout::new(
                    &self.session,
                    range,
                    self.presentation_width,
                )),
            });
        }
        self.patch_layout
            .as_ref()
            .map(|cached| cached.layout.clone())
    }

    fn patch_layout_key(&self, range: &std::ops::Range<usize>) -> u64 {
        let mut hasher = DefaultHasher::new();
        (Arc::as_ptr(self.document()) as usize).hash(&mut hasher);
        self.presentation_width.hash(&mut hasher);
        self.layout().is_split().hash(&mut hasher);
        range.start.hash(&mut hasher);
        range.end.hash(&mut hasher);
        for comment in self.review().comments() {
            comment.id.hash(&mut hasher);
            comment.anchor.hash(&mut hasher);
            comment.body.hash(&mut hasher);
            comment.outdated.hash(&mut hasher);
        }
        if let Some(draft) = self.session.draft() {
            draft.anchor().hash(&mut hasher);
            draft.body().hash(&mut hasher);
            draft.cursor().hash(&mut hasher);
        }
        hasher.finish()
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

fn offset(current: usize, delta: isize, last: usize) -> usize {
    if delta.is_negative() {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current.saturating_add(delta.unsigned_abs()).min(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diff_core::{FileDiff, ThemeId};

    #[test]
    fn new_uses_the_default_sage_theme() {
        let state = DiffReviewState::new(Arc::new(DiffDocument::empty()));
        assert_eq!(state.theme.id(), &ThemeId::Sage);
    }

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
        assert!(state.scroll > 0);
    }
}
