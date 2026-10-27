use crate::{
    DiffReviewCommand, InteractionPhase, KeyBinding, NavigationPane, ReviewOptions, ThemeChoice,
    default_diff_keybindings,
    patch_layout::{PatchContentLayout, PatchVisualLayout},
    render::{FILE_CHEVRON_WIDTH, GAP_EXPAND_WIDTH},
    theme_picker::ThemePicker,
};
use clankerdiff_client::{ClientState, ConnectionState};
use clankerdiff_core::{
    CommitPicker, DiffDocument, DiffPresentation, DiffScope, DiffSide, FileStatus, FileTree, Layout, RepositoryAction,
    RevealAmount, Review, ReviewCapabilities, ReviewSession, ReviewTarget, RowKind, ViewMode, ViewportAnchor,
};
use clankerdiff_syntax::{HighlightStats, SyntaxHighlighter};
use clankerdiff_theme::ReviewTheme;
use ratatui::layout::{Position, Rect};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

pub(crate) const SPLIT_BREAKPOINT: u16 = 96;

pub use clankerdiff_core::FocusPane;

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
    Commit { message: String },
    Discard { path: clankerdiff_core::RepoPath, status: FileStatus },
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

#[derive(Debug)]
struct CachedPatchContent {
    key: u64,
    layout: Arc<PatchContentLayout>,
}

#[derive(Debug)]
struct DiffViewportBookmark {
    scroll: usize,
    anchor: ViewportAnchor,
    segment: usize,
}

#[derive(Debug, Clone, Copy)]
enum PendingScroll {
    Follow,
    Jump(usize),
}

/// Persistent state for [`crate::DiffReviewWidget`].
#[derive(Debug)]
pub struct DiffReviewState {
    pub(crate) session: ReviewSession,
    pub(crate) options: ReviewOptions,
    pub(crate) capabilities: ReviewCapabilities,
    pub(crate) keybindings: Vec<KeyBinding<DiffReviewCommand>>,
    pub(crate) theme_choices: Arc<[ThemeChoice]>,
    pub(crate) theme: ReviewTheme,
    pub(crate) highlighter: SyntaxHighlighter,
    pub(crate) status: DiffReviewStatus,
    pub(crate) focus: FocusPane,
    pub(crate) drawer: FileTree,
    pub(crate) drawer_scroll: usize,
    pub(crate) drawer_height: usize,
    drawer_follow: DrawerFollow,
    pub(crate) scroll: usize,
    pub(crate) last_height: usize,
    pub(crate) presentation_width: u16,
    patch_content: Option<CachedPatchContent>,
    patch_layout: Option<CachedPatchLayout>,
    diff_viewport: Option<DiffViewportBookmark>,
    pending_scroll: Option<PendingScroll>,
    followed_file: Option<usize>,
    pub(crate) help: bool,
    pub(crate) help_scroll: usize,
    pub(crate) theme_picker: Option<ThemePicker>,
    pub(crate) commit_picker: Option<CommitPicker>,
    pub(crate) repository_prompt: Option<RepositoryPrompt>,
    pub(crate) repository_status: RepositoryOperationStatus,
    pub(crate) background_error: Option<String>,
    deferred_document: Option<Arc<DiffDocument>>,
    pub(crate) hit_layout: HitLayout,
    pub(crate) visible_rows: Vec<(u16, usize)>,
    pub(crate) cursor_position: Option<Position>,
    pub(crate) dirty: bool,
}

impl DiffReviewState {
    /// Creates ready state from an immutable document snapshot.
    #[must_use]
    pub fn new(document: Arc<DiffDocument>) -> Self {
        Self::with_theme(document, ReviewTheme::default())
    }

    /// Creates ready state with a shared neutral theme.
    #[must_use]
    pub fn with_theme(document: Arc<DiffDocument>, theme: ReviewTheme) -> Self {
        let mut drawer = FileTree::new(&document);
        let session = ReviewSession::new(document);
        drawer.reveal_file(session.selected_file().unwrap_or(0));
        let mut state = Self {
            session,
            options: ReviewOptions::default(),
            capabilities: ReviewCapabilities::default(),
            keybindings: default_diff_keybindings(),
            theme_choices: Arc::from([]),
            theme,
            highlighter: SyntaxHighlighter::default(),
            status: DiffReviewStatus::Ready,
            focus: FocusPane::Files,
            drawer,
            drawer_scroll: 0,
            drawer_height: 1,
            drawer_follow: DrawerFollow::Pending,
            scroll: 0,
            last_height: 0,
            presentation_width: 0,
            patch_content: None,
            patch_layout: None,
            diff_viewport: None,
            pending_scroll: Some(PendingScroll::Follow),
            followed_file: None,
            help: false,
            help_scroll: 0,
            theme_picker: None,
            commit_picker: None,
            repository_prompt: None,
            repository_status: RepositoryOperationStatus::Idle,
            background_error: None,
            deferred_document: None,
            hit_layout: HitLayout::default(),
            visible_rows: Vec::new(),
            cursor_position: None,
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

    #[must_use]
    pub const fn scope(&self) -> DiffScope {
        self.session.scope()
    }

    #[must_use]
    pub fn target(&self) -> &ReviewTarget {
        self.session.target()
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

    /// Returns the active renderer-neutral theme.
    #[must_use]
    pub const fn theme(&self) -> &ReviewTheme {
        &self.theme
    }

    /// Returns syntax cache and work counters.
    #[must_use]
    pub const fn highlight_stats(&self) -> HighlightStats {
        self.highlighter.stats()
    }

    /// Returns the first visible rendered-row index in the continuous diff.
    #[must_use]
    pub const fn scroll_offset(&self) -> usize {
        self.scroll
    }

    /// Returns the file whose section is at the top of the diff viewport.
    #[must_use]
    pub fn active_file(&mut self) -> Option<usize> {
        let source = self.patch_visual_layout()?.row(self.scroll)?.source();
        Some(self.presentation().row(source)?.file_index)
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

    /// Replaces the complete document while retaining and reconciling review comments.
    pub fn set_document(&mut self, document: Arc<DiffDocument>) {
        if self.interaction_phase() == InteractionPhase::Browse {
            self.install_document(document);
        } else {
            self.deferred_document = Some(document);
        }
    }

    pub(crate) fn install_deferred(&mut self) {
        if self.interaction_phase() != InteractionPhase::Browse {
            return;
        }
        if let Some(document) = self.deferred_document.take() {
            self.install_document(document);
        }
    }

    fn install_document(&mut self, document: Arc<DiffDocument>) {
        let bookmark = matches!(self.status, DiffReviewStatus::Ready).then(|| self.viewport_bookmark()).flatten();
        self.session.set_document(document);
        self.status = DiffReviewStatus::Ready;
        self.cursor_position = None;
        self.mark_dirty();
        let document = self.document().clone();
        self.drawer.rebuild(&document);
        if self.drawer.selected().is_none() {
            self.drawer.reveal_file(self.session.selected_file().unwrap_or(0));
        }
        self.follow_drawer_selection();
        match bookmark {
            Some(bookmark) => self.restore_viewport(&bookmark),
            None => self.request_follow(),
        }
    }

    /// Marks the state as waiting for a host snapshot.
    pub fn set_loading(&mut self) {
        self.status = DiffReviewStatus::Loading;
        self.deferred_document = None;
        self.session.cancel_draft();
        self.cursor_position = None;
        self.mark_dirty();
    }

    /// Shows a host-provided loading error.
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.status = DiffReviewStatus::Error(message.into());
        self.deferred_document = None;
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

    /// Clears a pending repository mutation without replacing the snapshot.
    pub fn clear_repository_pending(&mut self) {
        self.repository_status = RepositoryOperationStatus::Idle;
        self.mark_dirty();
    }

    /// Shows a repository mutation error while retaining the current snapshot.
    pub fn set_repository_error(&mut self, message: impl Into<String>) {
        self.repository_status = RepositoryOperationStatus::Error(message.into());
        self.repository_prompt = None;
        self.mark_dirty();
    }

    /// Reconciles background refresh health independently of command completion.
    pub fn set_background_error(&mut self, message: Option<String>) {
        if self.background_error != message {
            self.background_error = message;
            self.mark_dirty();
        }
    }

    pub fn apply_client_state(&mut self, client: &ClientState, installed: &mut Option<Arc<DiffDocument>>) {
        if let Some(snapshot) = client.snapshot_if_changed(installed) {
            self.set_document(Arc::clone(snapshot));
        }
        match client.status() {
            Some(error) if client.snapshot.is_none() && matches!(client.connection, ConnectionState::Failed(_)) => {
                self.set_error(error);
            }
            status => self.set_background_error(status),
        }
        self.set_capabilities(client.capabilities);
    }

    /// Whether a host command is still in flight.
    #[must_use]
    pub fn repository_pending(&self) -> bool {
        matches!(self.repository_status, RepositoryOperationStatus::Pending)
    }

    /// The command error, or otherwise the current background refresh failure.
    #[must_use]
    pub fn repository_error(&self) -> Option<&str> {
        match &self.repository_status {
            RepositoryOperationStatus::Error(message) => Some(message),
            _ => self.background_error.as_deref(),
        }
    }

    pub(crate) fn toggle_stage_action(&self) -> Option<RepositoryAction> {
        let path = &self.drawer.selected()?.path;
        FileTree::toggle_stage_action(self.document(), path)
    }

    pub(crate) fn begin_commit(&mut self) {
        self.repository_prompt = Some(RepositoryPrompt::Commit { message: String::new() });
    }

    pub(crate) fn begin_discard(&mut self) {
        let Some(file) = self.selected_file().and_then(|index| self.document().files.get(index)) else {
            return;
        };
        self.repository_prompt = Some(RepositoryPrompt::Discard { path: file.path.clone(), status: file.status });
    }

    pub fn set_theme(&mut self, theme: ReviewTheme) {
        self.theme_picker = None;
        self.apply_theme(theme);
    }

    pub(crate) fn apply_theme(&mut self, theme: ReviewTheme) {
        self.theme = theme;
        self.mark_dirty();
    }

    #[must_use]
    pub const fn options(&self) -> &ReviewOptions {
        &self.options
    }

    #[must_use]
    pub fn keybindings(&self) -> &[KeyBinding<DiffReviewCommand>] {
        &self.keybindings
    }

    pub fn set_keybindings(&mut self, bindings: impl Into<Vec<KeyBinding<DiffReviewCommand>>>) {
        self.keybindings = bindings.into();
        self.help_scroll = 0;
        self.mark_dirty();
    }

    pub fn set_options(&mut self, options: ReviewOptions) {
        if matches!(options.navigation, NavigationPane::Hidden | NavigationPane::Width(0)) {
            self.focus = FocusPane::Diff;
        }
        self.options = options;
        self.hit_layout = HitLayout::default();
        self.request_follow();
    }

    #[must_use]
    pub fn theme_choices(&self) -> &[ThemeChoice] {
        &self.theme_choices
    }

    pub fn set_theme_choices(&mut self, themes: impl Into<Arc<[ThemeChoice]>>) {
        if let Some(picker) = self.theme_picker.take() {
            self.set_theme(picker.cancel());
        }
        self.theme_choices = themes.into();
        self.mark_dirty();
    }

    pub(crate) fn select_file(&mut self, index: usize) -> bool {
        if !self.session.select_file(index) {
            return false;
        }
        self.drawer.reveal_file(index);
        self.follow_drawer_selection();
        self.scroll_to_selected_file();
        true
    }

    pub fn reveal_selected_gap(&mut self, amount: RevealAmount) -> bool {
        let previous = (self.session.selected_file_range(), self.session.selected_row(), self.session.selected_side());
        self.session.reveal_selected_gap(amount);
        let changed =
            previous != (self.session.selected_file_range(), self.session.selected_row(), self.session.selected_side());
        if changed {
            self.request_follow();
        }
        changed
    }

    pub fn toggle_source_view(&mut self) -> bool {
        let entering = self.session.source_view().is_none();
        let bookmark = entering.then(|| self.viewport_bookmark()).flatten();
        if !self.session.toggle_source_view() {
            return false;
        }
        if entering {
            self.diff_viewport = bookmark;
            self.scroll = 0;
            self.request_follow();
        } else if let Some(bookmark) = self.diff_viewport.take() {
            self.restore_viewport(&bookmark);
        }
        true
    }

    fn viewport_bookmark(&mut self) -> Option<DiffViewportBookmark> {
        let layout = self.patch_visual_layout()?;
        let index = layout.row(self.scroll)?.source();
        Some(DiffViewportBookmark {
            scroll: self.scroll,
            anchor: ViewportAnchor::capture(self.presentation(), index)?,
            segment: layout.focused_visual_row(index, false).map_or(0, |first| self.scroll.saturating_sub(first)),
        })
    }

    fn restore_viewport(&mut self, bookmark: &DiffViewportBookmark) {
        self.pending_scroll = None;
        self.mark_dirty();
        let Some(layout) = self.patch_visual_layout() else {
            return;
        };
        let target = bookmark
            .anchor
            .resolve(self.presentation(), self.session.visible_range())
            .and_then(|row| layout.focused_visual_row(row, false))
            .map(|first| first.saturating_add(bookmark.segment));
        self.scroll = target.unwrap_or(bookmark.scroll).min(layout.len().saturating_sub(self.last_height));
    }

    pub(crate) fn change_layout(&mut self, change: impl FnOnce(&mut Self) -> bool) -> bool {
        let bookmark = self.viewport_bookmark();
        let changed = change(self);
        if changed {
            match bookmark {
                Some(bookmark) => self.restore_viewport(&bookmark),
                None => self.request_follow(),
            }
        }
        changed
    }

    pub fn toggle_full_file(&mut self) -> bool {
        self.change_layout(|state| state.session.toggle_full_file())
    }

    pub fn toggle_file_collapsed(&mut self, index: usize) -> bool {
        self.change_layout(|state| state.session.toggle_file_collapsed(index))
    }

    pub fn toggle_all_files_collapsed(&mut self) -> bool {
        self.change_layout(|state| state.session.toggle_all_files_collapsed())
    }

    /// Selects automatic, unified, or split presentation.
    pub fn set_view_mode(&mut self, mode: ViewMode) {
        self.change_layout(|state| state.session.set_view_mode(mode));
    }

    /// Clears all queued comments and any active draft.
    pub fn clear_review(&mut self) {
        self.session.clear_review();
        self.cursor_position = None;
        self.mark_dirty();
    }

    pub(crate) fn ensure_presentation(&mut self, width: u16) {
        if self.presentation_width == width {
            return;
        }
        self.change_layout(|state| {
            state.presentation_width = width;
            state.session.set_split_when_auto(width >= SPLIT_BREAKPOINT);
            true
        });
    }

    pub(crate) fn scroll_to_selected_file(&mut self) {
        if let Some(file) = self.session.selected_file() {
            self.request_scroll(PendingScroll::Jump(file));
        }
    }

    pub(crate) fn move_drawer_entry(&mut self, delta: isize) {
        let file = self.drawer.move_selection(delta).and_then(|entry| entry.file);
        self.open_drawer_file(file);
    }

    pub(crate) fn select_drawer_entry(&mut self, position: usize) {
        if let Some(entry) = self.drawer.select(position) {
            let file = entry.file;
            self.open_drawer_file(file);
        }
    }

    fn open_drawer_file(&mut self, file: Option<usize>) {
        if let Some(file) = file
            && self.session.select_file(file)
        {
            self.scroll_to_selected_file();
        }
        self.follow_drawer_selection();
    }

    /// Expands the selected directory, or selects the current file. Returns
    /// whether the selected entry was a directory.
    pub(crate) fn expand_or_open_drawer_entry(&mut self) -> bool {
        match self.drawer.selected().cloned() {
            Some(entry) if entry.file.is_none() => {
                self.set_drawer_expanded(&entry.path, true);
                true
            }
            Some(entry) => {
                self.open_drawer_file(entry.file);
                false
            }
            None => false,
        }
    }

    pub(crate) fn collapse_drawer_entry(&mut self) {
        if let Some(entry) = self.drawer.selected().cloned()
            && entry.file.is_none()
        {
            self.set_drawer_expanded(&entry.path, false);
        }
    }

    fn set_drawer_expanded(&mut self, path: &str, expanded: bool) {
        self.drawer.set_expanded(path, expanded);
        self.follow_drawer_selection();
        self.mark_dirty();
    }

    pub(crate) fn follow_active_file(&mut self) {
        let active = self.active_file();
        if active == self.followed_file {
            return;
        }
        self.followed_file = active;
        if let Some(file) = active {
            self.drawer.follow_file(file);
            self.follow_drawer_selection();
        }
    }

    fn follow_drawer_selection(&mut self) {
        self.drawer_follow = DrawerFollow::Pending;
        let selected = self.drawer.selected_position().unwrap_or(0);
        if selected < self.drawer_scroll {
            self.drawer_scroll = selected;
        } else if selected >= self.drawer_scroll.saturating_add(self.drawer_height) {
            self.drawer_scroll = selected.saturating_sub(self.drawer_height.saturating_sub(1));
        }
    }

    pub(crate) fn take_drawer_follow_request(&mut self) -> bool {
        matches!(std::mem::replace(&mut self.drawer_follow, DrawerFollow::Settled), DrawerFollow::Pending)
    }

    pub(crate) fn move_row(&mut self, delta: isize) {
        if self.session.move_row(delta) {
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
        let clamped = target.min(layout.len().saturating_sub(self.last_height.max(1)));
        self.pending_scroll = None;
        if clamped != self.scroll {
            self.scroll = clamped;
            self.mark_dirty();
        }
    }

    /// Scrolls the file drawer without moving the selected file.
    pub(crate) fn scroll_drawer(&mut self, delta: isize) {
        let last = self.drawer.entries().len().saturating_sub(self.drawer_height);
        let target = if delta.is_negative() {
            self.drawer_scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.drawer_scroll.saturating_add(delta.unsigned_abs())
        };
        let clamped = target.min(last);
        self.drawer_follow = DrawerFollow::Settled;
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
        self.request_scroll(PendingScroll::Follow);
    }

    fn request_scroll(&mut self, scroll: PendingScroll) {
        self.pending_scroll = Some(scroll);
        self.mark_dirty();
        self.apply_scroll(scroll);
    }

    pub(crate) fn apply_pending_scroll(&mut self) {
        if let Some(scroll) = self.pending_scroll.take() {
            self.apply_scroll(scroll);
        }
    }

    fn apply_scroll(&mut self, scroll: PendingScroll) {
        match scroll {
            PendingScroll::Follow => self.follow_selection(),
            PendingScroll::Jump(file) => {
                if let Some(layout) = self.patch_visual_layout()
                    && let Some(start) = self.presentation().file_range(file)
                    && let Some(row) = layout.focused_visual_row(start.start, false)
                {
                    self.scroll = row.min(layout.len().saturating_sub(self.last_height.max(1)));
                }
            }
        }
    }

    fn follow_selection(&mut self) {
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
        let top = target.saturating_sub(usize::from(height > 1));
        if top < self.scroll {
            self.scroll = top;
        } else if target >= self.scroll.saturating_add(height) {
            self.scroll = target.saturating_sub(height.saturating_sub(1));
        }
        self.scroll = self.scroll.min(layout.len().saturating_sub(height));
    }

    pub(crate) fn patch_visual_layout(&mut self) -> Option<Arc<PatchVisualLayout>> {
        self.session.selected_file()?;
        let content_key = self.patch_content_key();
        if self.patch_content.as_ref().is_none_or(|cached| cached.key != content_key) {
            self.patch_content = Some(CachedPatchContent {
                key: content_key,
                layout: Arc::new(PatchContentLayout::new(
                    self.session.presentation(),
                    self.session.visible_range(),
                    self.presentation_width,
                    self.options.tab_width,
                )),
            });
        }
        let content = self.patch_content.as_ref()?.layout.clone();
        let key = self.patch_annotation_key(content_key);
        if self.patch_layout.as_ref().is_none_or(|cached| cached.key != key) {
            self.patch_layout = Some(CachedPatchLayout {
                key,
                layout: Arc::new(PatchVisualLayout::new(&self.session, content, self.presentation_width)),
            });
        }
        self.patch_layout.as_ref().map(|cached| cached.layout.clone())
    }

    fn patch_content_key(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        (Arc::as_ptr(self.document()) as usize).hash(&mut hasher);
        self.presentation_width.hash(&mut hasher);
        self.layout().is_split().hash(&mut hasher);
        self.options.tab_width.hash(&mut hasher);
        self.session.projection_revision().hash(&mut hasher);
        self.session.visible_range().hash(&mut hasher);
        hasher.finish()
    }

    fn patch_annotation_key(&self, content_key: u64) -> u64 {
        let mut hasher = DefaultHasher::new();
        content_key.hash(&mut hasher);
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

    pub(crate) fn select_clicked_row(&mut self, position: Position) -> bool {
        let clicked =
            self.visible_rows.iter().find(|(screen_row, _)| *screen_row == position.y).map(|(_, index)| *index);
        if let Some(row) =
            clicked.and_then(|index| self.presentation().row(index)).filter(|row| row.kind == RowKind::FileHeader)
        {
            let file = row.file_index;
            if position.x < self.hit_layout.patch.x.saturating_add(FILE_CHEVRON_WIDTH)
                && self.session.source_view().is_none()
            {
                self.toggle_file_collapsed(file);
                return false;
            }
            return self.select_file(file);
        }
        if let Some(index) = clicked
            && self.session.select_row(index)
        {
            if self.session.presentation().gap_info(index).is_some() {
                if position.x < self.hit_layout.patch.x.saturating_add(GAP_EXPAND_WIDTH) {
                    self.reveal_selected_gap(RevealAmount::Step);
                } else {
                    self.mark_dirty();
                }
            } else {
                self.mark_dirty();
                self.pending_scroll = None;
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clankerdiff_core::FileDiff;
    use clankerdiff_theme::ThemeId;

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
            ..DiffDocument::default()
        });
        let mut state = DiffReviewState::new(document);
        state.last_height = 3;
        let anchor = state.session.selected_anchor().unwrap();
        state.session.review_mut().add_comment(anchor, "note");
        state.move_row(2);
        state.follow_selection();
        assert!(state.scroll > 0);
    }
}
