#![allow(missing_docs)] // GPUI's `actions!` macro cannot attach per-action rustdoc.

mod commands;

use crate::{
    DEFAULT_FONT_FAMILY, DEFAULT_FONT_SIZE, DiffViewerEvent, ThemeChanged,
    comment_editor::{CommentEditor, CommentEditorEvent},
    sidebar::SidebarResizeDrag,
    style::color,
    ui::prelude::{Modal, Notification, ThemePicker, ThemePickerItem, UiTheme},
};
use clankerdiff_core::{
    DiffDocument, DiffPresentation, DiffScope, DiffSide, FileStatus, FileTree, Layout, LineAnchor,
    PresentedCell, PresentedRow, RepoPath, RepositoryAction, RevealAmount, Review, ReviewSession,
    StageState, ViewMode, ViewportAnchor,
};
use clankerdiff_core::{DiffReviewCommand, ReviewCapabilities, ReviewCommand};
use clankerdiff_syntax::{
    HighlightSpan, HighlightStats, LanguageHint, SyntaxHighlighter, empty_spans,
};
use clankerdiff_theme::ReviewTheme;
use clankerdiff_theme::ThemeSelection;
use gpui::{
    App, Context, DragMoveEvent, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding,
    KeyContext, ListAlignment, ListOffset, ListState, ScrollHandle, Subscription, Window, actions,
    div, prelude::*, px,
};
use std::sync::Arc;

const MIN_SIDEBAR_WIDTH: f32 = 180.0;
const MAX_SIDEBAR_WIDTH: f32 = 960.0;
const DEFAULT_SIDEBAR_FRACTION: f32 = 0.25;
const DEFAULT_MIN_SIDEBAR_WIDTH: f32 = 320.0;
const MIN_DIFF_WIDTH: f32 = 320.0;
const SIDEBAR_DIVIDER_WIDTH: f32 = 1.0;
const MIN_FONT_SIZE: f32 = 10.0;
const MAX_FONT_SIZE: f32 = 24.0;
const FONT_SIZE_STEP: f32 = 1.0;
const DIFF_ROW_VERTICAL_SPACE: f32 = 7.0;
const SIDEBAR_ROW_VERTICAL_SPACE: f32 = 20.0;

const BROWSE_ACTIVATION: &str =
    "DiffViewer && mode == browse && !CommentDismissControl && !StageCheckbox";
const DIFF_ACTIVATION: &str =
    "DiffViewer && mode == browse && pane == diff && !CommentDismissControl && !StageCheckbox";
const FILES_ACTIVATION: &str =
    "DiffViewer && mode == browse && pane == files && !CommentDismissControl && !StageCheckbox";

actions!(
    diff_viewer,
    [
        NextFile,
        PreviousFile,
        NextHunk,
        PreviousHunk,
        NextItem,
        PreviousItem,
        FirstItem,
        LastItem,
        PageUp,
        PageDown,
        TogglePane,
        FocusFiles,
        FocusDiff,
        SelectOldSide,
        SelectNewSide,
        ExpandOrOpen,
        Collapse,
        CycleViewMode,
        ExpandGap,
        ActivateGap,
        ExpandGapAll,
        ToggleFullFile,
        ToggleSourceView,
        AddComment,
        EditComment,
        DeleteComment,
        UndoComment,
        SubmitComment,
        CancelComment,
        CopyReview,
        SubmitReview,
        ShowShortcuts,
        HideShortcuts,
        ShowThemePicker,
        HideThemePicker,
        NextTheme,
        PreviousTheme,
        CommitTheme,
        Refresh,
        IncreaseFontSize,
        DecreaseFontSize,
        ResetFontSize,
        ToggleStage,
        StageAll,
        UnstageAll,
        CycleScope,
        CommitChanges,
        DiscardChanges,
        ConfirmDiscard,
        CancelRepositoryPrompt,
        Cancel
    ]
);

/// Renderer-specific sizing and virtualization settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffViewerOptions {
    /// Initial sidebar width in logical pixels, or None for responsive sizing.
    pub sidebar_width: Option<f32>,
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
            sidebar_width: None,
            font_size: DEFAULT_FONT_SIZE,
            row_height: 20.0,
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

#[derive(Debug, Clone)]
enum RepositoryPrompt {
    Commit,
    Discard { path: RepoPath, status: FileStatus },
}

pub use clankerdiff_core::FocusPane as ViewerPane;

struct DiffViewportBookmark {
    offset: ListOffset,
    anchor: Option<ViewportAnchor>,
}

/// Shared GPUI diff review view.
///
pub struct DiffViewer {
    session: ReviewSession,
    capabilities: ReviewCapabilities,
    theme_selection: Option<ThemeSelection>,
    scope: DiffScope,
    theme: ReviewTheme,
    highlighter: SyntaxHighlighter,
    options: DiffViewerOptions,
    sidebar_width: f32,
    preferred_sidebar_width: Option<f32>,
    font_size: f32,
    pub(crate) diff_list_state: ListState,
    diff_list_revision: Option<u64>,
    followed_file: Option<usize>,
    diff_list_split: bool,
    diff_viewport: Option<DiffViewportBookmark>,
    pub(crate) sidebar_tree: FileTree,
    pub(crate) sidebar_scroll_handle: ScrollHandle,
    pub(crate) pane: ViewerPane,
    shortcuts_open: bool,
    pub(crate) comment_target: Option<CommentTarget>,
    pub(crate) comment_editor: Option<Entity<CommentEditor>>,
    comment_editor_subscription: Option<Subscription>,
    comment_editor_focus: Option<FocusHandle>,
    repository_prompt: Option<RepositoryPrompt>,
    repository_editor: Option<Entity<CommentEditor>>,
    repository_editor_subscription: Option<Subscription>,
    repository_error: Option<String>,
    background_error: Option<String>,
    repository_pending: bool,
    focus_handle: Option<gpui::FocusHandle>,
}

impl DiffViewer {
    /// Creates a viewer with the default Sage theme and options.
    #[must_use]
    pub fn new(document: Arc<DiffDocument>) -> Self {
        Self::with_options(
            document,
            ReviewTheme::default(),
            DiffViewerOptions::default(),
        )
    }

    /// Creates a viewer with explicit theme and adapter options.
    #[must_use]
    pub fn with_options(
        document: Arc<DiffDocument>,
        theme: ReviewTheme,
        options: DiffViewerOptions,
    ) -> Self {
        let mut sidebar_tree = FileTree::new(&document);
        let sidebar_width = options
            .sidebar_width
            .unwrap_or(DEFAULT_MIN_SIDEBAR_WIDTH)
            .clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
        let font_size = clamp_font_size(options.font_size);
        let row_height = effective_diff_row_height(options.row_height, font_size);
        let session = ReviewSession::new(document);
        sidebar_tree.reveal_file(session.selected_file().unwrap_or(0));
        let mut viewer = Self {
            session,
            scope: DiffScope::Both,
            capabilities: ReviewCapabilities::default(),
            theme_selection: None,
            theme,
            highlighter: SyntaxHighlighter::new(options.highlight_cache_capacity),
            options,
            sidebar_width,
            preferred_sidebar_width: options.sidebar_width,
            font_size,
            diff_list_state: ListState::new(0, ListAlignment::Top, px(row_height * 8.0)),
            diff_list_revision: None,
            followed_file: None,
            diff_list_split: false,
            diff_viewport: None,
            sidebar_tree,
            sidebar_scroll_handle: ScrollHandle::new(),
            pane: ViewerPane::Diff,
            shortcuts_open: false,
            comment_target: None,
            comment_editor: None,
            comment_editor_subscription: None,
            comment_editor_focus: None,
            repository_prompt: None,
            repository_editor: None,
            repository_editor_subscription: None,
            repository_error: None,
            background_error: None,
            repository_pending: false,
            focus_handle: None,
        };
        viewer.sync_diff_list();
        viewer
    }

    /// Installs the actions' default keyboard shortcuts on an application.
    pub fn bind_keys(cx: &mut App) {
        const BROWSE: &str = "DiffViewer && mode == browse";
        const DIFF: &str = "DiffViewer && mode == browse && pane == diff";
        const DIFF_SPLIT: &str = "DiffViewer && mode == browse && pane == diff && layout == split";
        const DIFF_UNIFIED: &str =
            "DiffViewer && mode == browse && pane == diff && layout == unified";
        const FILES: &str = "DiffViewer && mode == browse && pane == files";
        const DRAFT: &str = "DiffViewer && mode == draft";
        const SHORTCUTS: &str = "DiffViewer && mode == shortcuts";
        cx.bind_keys([
            KeyBinding::new("down", NextItem, Some(BROWSE)),
            KeyBinding::new("j", NextItem, Some(BROWSE)),
            KeyBinding::new("up", PreviousItem, Some(BROWSE)),
            KeyBinding::new("k", PreviousItem, Some(BROWSE)),
            KeyBinding::new("home", FirstItem, Some(BROWSE)),
            KeyBinding::new("g", FirstItem, Some(BROWSE)),
            KeyBinding::new("end", LastItem, Some(BROWSE)),
            KeyBinding::new("shift-g", LastItem, Some(BROWSE)),
            KeyBinding::new("pageup", PageUp, Some(BROWSE)),
            KeyBinding::new("pagedown", PageDown, Some(BROWSE)),
            KeyBinding::new("tab", TogglePane, Some(BROWSE_ACTIVATION)),
            KeyBinding::new("h", FocusFiles, Some(DIFF)),
            KeyBinding::new("left", SelectOldSide, Some(DIFF_SPLIT)),
            KeyBinding::new("left", FocusFiles, Some(DIFF_UNIFIED)),
            KeyBinding::new("right", SelectNewSide, Some(DIFF_SPLIT)),
            KeyBinding::new("h", Collapse, Some(FILES)),
            KeyBinding::new("left", Collapse, Some(FILES)),
            KeyBinding::new("l", ExpandOrOpen, Some(FILES)),
            KeyBinding::new("right", ExpandOrOpen, Some(FILES)),
            KeyBinding::new("enter", ExpandOrOpen, Some(FILES_ACTIVATION)),
            KeyBinding::new("space", ToggleStage, Some(FILES_ACTIVATION)),
            KeyBinding::new("a", StageAll, Some(FILES)),
            KeyBinding::new("shift-a", UnstageAll, Some(FILES)),
            KeyBinding::new("shift-c", CommitChanges, Some(BROWSE)),
            KeyBinding::new("d", DiscardChanges, Some(BROWSE)),
            KeyBinding::new(
                "y",
                ConfirmDiscard,
                Some("DiffViewer && mode == repository-discard"),
            ),
            KeyBinding::new(
                "n",
                CancelRepositoryPrompt,
                Some("DiffViewer && mode == repository-discard"),
            ),
            KeyBinding::new(
                "escape",
                CancelRepositoryPrompt,
                Some("DiffViewer && mode == repository-discard"),
            ),
            KeyBinding::new("c", AddComment, Some(DIFF)),
            KeyBinding::new("e", EditComment, Some(DIFF)),
            KeyBinding::new("x", DeleteComment, Some(DIFF)),
            KeyBinding::new("u", UndoComment, Some(DIFF)),
            KeyBinding::new("s", SubmitReview, Some(DIFF)),
            KeyBinding::new("y", CopyReview, Some(DIFF)),
            KeyBinding::new("o", ToggleSourceView, Some(BROWSE)),
            KeyBinding::new("shift-o", ExpandGapAll, Some(DIFF)),
            KeyBinding::new("f", ToggleFullFile, Some(DIFF)),
            KeyBinding::new("enter", ExpandGap, Some(DIFF_ACTIVATION)),
            KeyBinding::new("v", CycleViewMode, Some(BROWSE)),
            KeyBinding::new("shift-s", CycleScope, Some(BROWSE)),
            KeyBinding::new("shift-/", ShowShortcuts, Some(BROWSE)),
            KeyBinding::new("escape", HideShortcuts, Some(SHORTCUTS)),
            KeyBinding::new("shift-/", HideShortcuts, Some(SHORTCUTS)),
            KeyBinding::new("t", ShowThemePicker, Some(BROWSE)),
            KeyBinding::new("cmd-shift-t", ShowThemePicker, Some(BROWSE)),
            KeyBinding::new("ctrl-shift-t", ShowThemePicker, Some(BROWSE)),
            KeyBinding::new(
                "escape",
                HideThemePicker,
                Some("DiffViewer && mode == themes"),
            ),
            KeyBinding::new("down", NextTheme, Some("DiffViewer && mode == themes")),
            KeyBinding::new("j", NextTheme, Some("DiffViewer && mode == themes")),
            KeyBinding::new("up", PreviousTheme, Some("DiffViewer && mode == themes")),
            KeyBinding::new("k", PreviousTheme, Some("DiffViewer && mode == themes")),
            KeyBinding::new("enter", CommitTheme, Some("DiffViewer && mode == themes")),
            KeyBinding::new("cmd-r", Refresh, Some(BROWSE)),
            KeyBinding::new("ctrl-r", Refresh, Some(BROWSE)),
            KeyBinding::new("escape", Cancel, Some(BROWSE)),
            KeyBinding::new("ctrl-g", Cancel, Some(BROWSE)),
            KeyBinding::new("cmd-]", NextFile, Some(BROWSE)),
            KeyBinding::new("cmd-[", PreviousFile, Some(BROWSE)),
            KeyBinding::new("cmd-enter", SubmitComment, Some(DRAFT)),
            KeyBinding::new("ctrl-enter", SubmitComment, Some(DRAFT)),
            KeyBinding::new("escape", CancelComment, Some(DRAFT)),
            KeyBinding::new("cmd-shift-c", CopyReview, Some(BROWSE)),
            KeyBinding::new("cmd-shift-enter", SubmitReview, Some(BROWSE)),
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
    pub const fn scope(&self) -> DiffScope {
        self.scope
    }

    pub fn set_scope(&mut self, scope: DiffScope, cx: &mut Context<Self>) {
        self.scope = scope;
        cx.notify();
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
    pub const fn theme(&self) -> &ReviewTheme {
        &self.theme
    }

    /// Returns semantic component tokens for the current theme.
    pub(crate) fn ui_theme(&self) -> UiTheme {
        UiTheme::new(&self.theme)
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

    /// Returns the pane that receives browse-mode navigation commands.
    #[must_use]
    pub const fn pane(&self) -> ViewerPane {
        self.pane
    }

    /// Returns whether the keyboard shortcut reference is open.
    #[must_use]
    pub const fn shortcuts_open(&self) -> bool {
        self.shortcuts_open
    }

    /// Returns whether the theme picker is open.
    #[must_use]
    pub const fn theme_picker_open(&self) -> bool {
        self.theme_selection.is_some()
    }

    /// Returns the current diff list scroll position.
    #[must_use]
    pub fn diff_scroll_top(&self) -> ListOffset {
        self.diff_list_state.logical_scroll_top()
    }

    pub fn scroll_diff_to_end(&mut self, cx: &mut Context<Self>) {
        self.diff_list_state.scroll_to_end();
        cx.notify();
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

    pub fn set_document(&mut self, document: Arc<DiffDocument>, cx: &mut Context<Self>) {
        self.sidebar_tree.rebuild(&document);
        self.change_layout(|session| {
            session.set_document(document);
            true
        });
        if self.sidebar_tree.selected().is_none() {
            self.sidebar_tree
                .reveal_file(self.session.selected_file().unwrap_or(0));
        }
        self.diff_list_state.remeasure();
        cx.notify();
    }

    fn settle_comment_editor(&mut self) {
        if let Some(draft) = self.session.draft() {
            let row = self.presentation().row_showing_anchor(draft.anchor());
            if let (Some(row), Some(target)) = (row, self.comment_target.as_mut()) {
                target.row_index = row;
                self.set_comment_row_focus(row, self.comment_editor_focus.clone());
            }
        } else {
            self.clear_comment_editor();
        }
    }

    pub(crate) fn change_layout(
        &mut self,
        change: impl FnOnce(&mut ReviewSession) -> bool,
    ) -> bool {
        let bookmark = self.diff_viewport_bookmark();
        let changed = change(&mut self.session);
        if self.sync_diff_list() {
            self.restore_diff_viewport(bookmark);
        }
        changed
    }

    pub(crate) fn expand_selected_gap(&mut self, amount: RevealAmount, cx: &mut Context<Self>) {
        if self.change_layout(|session| session.reveal_selected_gap(amount)) {
            cx.notify();
        }
    }

    /// Marks a repository operation as pending.
    pub fn set_repository_pending(&mut self, pending: bool, cx: &mut Context<Self>) {
        self.repository_pending = pending;
        if pending {
            self.repository_error = None;
            self.clear_repository_prompt();
        }
        cx.notify();
    }

    /// Shows a repository error without replacing the current document.
    pub fn set_repository_error(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.repository_pending = false;
        self.repository_error = Some(message.into());
        self.clear_repository_prompt();
        cx.notify();
    }

    /// Reconciles background refresh health without settling a user command.
    pub fn set_background_error(&mut self, message: Option<String>, cx: &mut Context<Self>) {
        if self.background_error != message {
            self.background_error = message;
            cx.notify();
        }
    }

    /// Whether a host command is still in flight.
    #[must_use]
    pub const fn repository_pending(&self) -> bool {
        self.repository_pending
    }

    /// The command error, or otherwise the current background refresh failure.
    #[must_use]
    pub fn repository_error(&self) -> Option<&str> {
        self.repository_error
            .as_deref()
            .or(self.background_error.as_deref())
    }

    pub fn set_theme(&mut self, theme: ReviewTheme, cx: &mut Context<Self>) {
        self.theme_selection = None;
        self.apply_theme(theme, cx);
    }

    fn apply_theme(&mut self, theme: ReviewTheme, cx: &mut Context<Self>) {
        self.theme = theme.clone();
        if let Some(editor) = &self.comment_editor {
            editor.update(cx, |editor, cx| editor.set_theme(theme, cx));
        }
        self.diff_list_state.remeasure();
        cx.notify();
    }

    /// Selects automatic, unified, or split layout.
    pub fn set_view_mode(&mut self, mode: ViewMode, cx: &mut Context<Self>) {
        if self.change_layout(|session| session.set_view_mode(mode)) {
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

    pub(crate) fn dismiss_comment(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .session
            .draft()
            .is_some_and(|draft| draft.editing() == Some(id))
        {
            self.discard_comment(cx);
        }
        self.remove_comment(id, cx);
        if let Some(editor) = &self.comment_editor {
            editor.focus_handle(cx).focus(window, cx);
        } else if let Some(focus) = &self.focus_handle {
            focus.focus(window, cx);
        }
        cx.notify();
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
        self.preferred_sidebar_width = self.options.sidebar_width;
        cx.notify();
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
        self.preferred_sidebar_width = Some(width);
        if (self.sidebar_width - width).abs() <= f32::EPSILON {
            return;
        }
        self.sidebar_width = width;
        self.diff_list_state.remeasure();
        cx.notify();
    }

    #[must_use]
    pub fn active_file(&self) -> Option<usize> {
        let range = self.session.visible_range();
        let row = range.start + self.diff_list_state.logical_scroll_top().item_ix;
        Some(
            self.presentation()
                .row(row.min(range.end.checked_sub(1)?))?
                .file_index,
        )
    }

    pub(crate) fn list_item(&self, row: usize) -> Option<usize> {
        let range = self.session.visible_range();
        range.contains(&row).then(|| row - range.start)
    }

    fn follow_active_file(&mut self) {
        let active = self.active_file();
        if active == self.followed_file {
            return;
        }
        self.followed_file = active;
        if let Some(file) = active {
            self.sidebar_tree.follow_file(file);
            self.reveal_sidebar_selection();
        }
    }

    pub(crate) fn sync_diff_list(&mut self) -> bool {
        let revision = self.session.projection_revision();
        let rebuild = self.diff_list_revision != Some(revision);
        if rebuild {
            self.diff_list_state.reset_with_uniform_height(
                self.session.visible_range().len(),
                px(self.diff_row_height()),
            );
            self.diff_list_revision = Some(revision);
            self.settle_comment_editor();
        }
        if self.diff_list_split != self.layout().is_split() {
            self.diff_list_state.remeasure();
            self.diff_list_split = self.layout().is_split();
        }
        rebuild
    }

    fn diff_viewport_bookmark(&self) -> DiffViewportBookmark {
        let offset = self.diff_list_state.logical_scroll_top();
        let row = self.session.visible_range().start + offset.item_ix;
        DiffViewportBookmark {
            offset,
            anchor: ViewportAnchor::capture(self.presentation(), row),
        }
    }

    fn restore_diff_viewport(&self, bookmark: DiffViewportBookmark) {
        let range = self.session.visible_range();
        self.diff_list_state.scroll_to(
            match bookmark
                .anchor
                .and_then(|anchor| anchor.resolve(self.presentation(), range.clone()))
            {
                Some(row) => ListOffset {
                    item_ix: row - range.start,
                    ..bookmark.offset
                },
                None => ListOffset {
                    item_ix: bookmark.offset.item_ix.min(range.len().saturating_sub(1)),
                    offset_in_item: px(0.0),
                },
            },
        );
    }

    pub(crate) fn select_file(&mut self, index: usize, cx: &mut Context<Self>) {
        self.sidebar_tree.reveal_file(index);
        if self.session.select_file(index) {
            self.sync_diff_list();
            self.clear_comment_editor();
            if let Some(item_ix) = self
                .presentation()
                .file_range(index)
                .and_then(|range| self.list_item(range.start))
            {
                self.diff_list_state.scroll_to(ListOffset {
                    item_ix,
                    offset_in_item: px(0.0),
                });
            }
            cx.notify();
        }
    }

    pub(crate) fn toggle_stage_path(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.repository_pending {
            return;
        }
        let state = FileTree::stage_state_for(self.document(), path);
        let paths = FileTree::paths_for(self.document(), path);
        if paths.is_empty() {
            return;
        }
        let action = if state == StageState::Staged {
            RepositoryAction::UnstagePaths(paths)
        } else {
            RepositoryAction::StagePaths(paths)
        };
        self.handle_command(DiffReviewCommand::RepositoryAction(action), window, cx);
    }

    pub(crate) fn toggle_directory(
        &mut self,
        position: usize,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.handle_command(DiffReviewCommand::Focus(ViewerPane::Files), window, cx) {
            return;
        }
        self.sidebar_tree.select(position);
        self.sidebar_tree.toggle(path);
        cx.notify();
    }

    pub(crate) fn select_diff_cell(
        &mut self,
        row_index: usize,
        side: DiffSide,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.handle_command(DiffReviewCommand::SelectRow(row_index), window, cx) {
            self.handle_command(DiffReviewCommand::SelectSide(side), window, cx);
            self.handle_command(DiffReviewCommand::Focus(ViewerPane::Diff), window, cx);
        }
    }

    pub(crate) fn begin_comment(
        &mut self,
        row_index: usize,
        side: DiffSide,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_comment_editor(row_index, side, None, window, cx);
    }

    fn open_comment_editor(
        &mut self,
        row_index: usize,
        side: DiffSide,
        editing: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = if editing.is_some() {
            ReviewCommand::EditComment
        } else {
            ReviewCommand::BeginComment
        };
        if !self.command_enabled(&command.into()) {
            return;
        }
        if !self.handle_command(DiffReviewCommand::SelectRow(row_index), window, cx) {
            return;
        }
        self.handle_command(DiffReviewCommand::SelectSide(side), window, cx);
        self.handle_command(command, window, cx);
    }

    fn mount_comment_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row_index) = self.session.selected_row() else {
            return;
        };
        let side = self.session.selected_side();
        let previous_row = self.comment_target.as_ref().map(|target| target.row_index);
        let body = self
            .session
            .draft()
            .map_or_else(String::new, |draft| draft.body().to_owned());
        let editor = cx.new(|cx| CommentEditor::new(body, self.theme.clone(), window, cx));
        self.comment_editor_subscription = Some(cx.subscribe_in(
            &editor,
            window,
            |viewer, _editor, event: &CommentEditorEvent, window, cx| match event {
                CommentEditorEvent::Changed(body) => {
                    if let Some(draft) = viewer.session.draft_mut() {
                        draft.set_body(body);
                    }
                    viewer.remeasure_comment_row();
                    cx.notify();
                }
                CommentEditorEvent::Submit => {
                    viewer.handle_command(ReviewCommand::SubmitComment, window, cx);
                }
                CommentEditorEvent::Cancel => {
                    viewer.handle_command(ReviewCommand::Cancel, window, cx);
                }
            },
        ));
        if let Some(previous_row) = previous_row {
            self.set_comment_row_focus(previous_row, None);
        }
        self.pane = ViewerPane::Diff;
        self.comment_target = Some(CommentTarget { row_index, side });
        self.comment_editor = Some(editor.clone());
        self.remeasure_row(previous_row);
        self.remeasure_row(Some(row_index));
        let focus = editor.focus_handle(cx);
        self.comment_editor_focus = Some(focus.clone());
        self.set_comment_row_focus(row_index, Some(focus.clone()));
        focus.focus(window, cx);
        cx.notify();
    }

    fn set_comment_row_focus(&self, row: usize, focus: Option<FocusHandle>) {
        if let Some(item) = self
            .list_item(row)
            .filter(|&item| item < self.diff_list_state.item_count())
        {
            let offset = self.diff_list_state.logical_scroll_top();
            self.diff_list_state
                .splice_focusable(item..item + 1, [focus]);
            self.diff_list_state.scroll_to(offset);
        }
    }

    fn remeasure_row(&self, row_index: Option<usize>) {
        if let Some(item) = row_index.and_then(|row| self.list_item(row)) {
            self.diff_list_state.remeasure_items(item..item + 1);
        }
    }

    fn remeasure_comment_row(&self) {
        self.remeasure_row(self.comment_target.as_ref().map(|target| target.row_index));
    }

    fn clear_comment_editor(&mut self) {
        if let Some(target) = &self.comment_target {
            self.set_comment_row_focus(target.row_index, None);
        }
        self.comment_editor_focus = None;
        self.comment_target = None;
        self.comment_editor = None;
        self.comment_editor_subscription = None;
    }

    fn finish_comment(&mut self, cx: &mut Context<Self>) {
        self.session.submit_draft();
        let row = self.comment_target.as_ref().map(|target| target.row_index);
        self.clear_comment_editor();
        self.remeasure_row(row);
        cx.notify();
    }

    fn discard_comment(&mut self, cx: &mut Context<Self>) {
        self.session.cancel_draft();
        let row = self.comment_target.as_ref().map(|target| target.row_index);
        self.clear_comment_editor();
        self.remeasure_row(row);
        cx.notify();
    }

    fn expand_gap_action(&mut self, _: &ExpandGap, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::RevealGap(RevealAmount::Step), window, cx);
    }

    fn activate_gap_action(
        &mut self,
        _: &ActivateGap,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected_gap = self
            .session
            .selected_row()
            .is_some_and(|row| self.session.presentation().gap_info(row).is_some());
        if selected_gap {
            self.handle_command(DiffReviewCommand::RevealGap(RevealAmount::Step), window, cx);
        }
    }

    fn expand_gap_all_action(
        &mut self,
        _: &ExpandGapAll,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_command(DiffReviewCommand::RevealGap(RevealAmount::All), window, cx);
    }

    fn toggle_source_view_action(
        &mut self,
        _: &ToggleSourceView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_command(DiffReviewCommand::ToggleSourceView, window, cx);
    }

    pub fn toggle_source_view(&mut self, cx: &mut Context<Self>) -> bool {
        let entering = self.session.source_view().is_none();
        let bookmark = self.diff_viewport_bookmark();
        if !self.session.toggle_source_view() {
            return false;
        }
        self.sync_diff_list();
        if entering {
            self.diff_viewport = Some(bookmark);
            self.reveal_selected_row();
        } else if let Some(bookmark) = self.diff_viewport.take() {
            self.restore_diff_viewport(bookmark);
        }
        cx.notify();
        true
    }

    fn toggle_full_file_action(
        &mut self,
        _: &ToggleFullFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_command(DiffReviewCommand::ToggleFullFile, window, cx);
    }

    pub(crate) fn highlight_cell(
        &mut self,
        row: &PresentedRow,
        cell: &PresentedCell,
    ) -> Arc<[HighlightSpan]> {
        let source = self.session.presentation().cell_context(row, cell);
        self.highlighter
            .with_theme(&self.theme.syntax)
            .highlight_document(source.id, LanguageHint::Path(source.path), || source.text())
            .ok()
            .and_then(|highlights| highlights.line_shared(source.target_line))
            .unwrap_or_else(empty_spans)
    }

    fn move_file(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(current) = self.selected_file() else {
            return;
        };
        if let Some(index) = self.presentation().offset_file(current, delta) {
            self.handle_command(DiffReviewCommand::SelectFile(index), window, cx);
        }
    }

    fn move_hunk(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.session.hunk_target(delta) {
            self.handle_command(DiffReviewCommand::SelectRow(index), window, cx);
        }
    }

    fn reveal_selected_row(&self) {
        let Some(item) = self
            .session
            .selected_row()
            .and_then(|row| self.list_item(row))
        else {
            return;
        };
        let divider_height = px(self.file_divider_height());
        let under_divider = item <= self.diff_list_state.logical_scroll_top().item_ix
            || self
                .diff_list_state
                .bounds_for_item(item)
                .is_some_and(|bounds| {
                    bounds.top() < self.diff_list_state.viewport_bounds().top() + divider_height
                });
        if under_divider {
            self.diff_list_state.scroll_to(ListOffset {
                item_ix: item,
                offset_in_item: px(0.0),
            });
            self.diff_list_state.scroll_by(-divider_height);
        } else {
            self.diff_list_state.scroll_to_reveal_item(item);
        }
    }

    fn move_item(&mut self, delta: isize, cx: &mut Context<Self>) {
        match self.pane {
            ViewerPane::Diff => {
                self.session.move_row(delta);
                self.reveal_selected_row();
            }
            ViewerPane::Files => {
                let file = self
                    .sidebar_tree
                    .move_selection(delta)
                    .map(|entry| entry.file);
                self.open_sidebar_file(file.flatten(), cx);
            }
        }
        cx.notify();
    }

    fn reveal_sidebar_selection(&self) {
        if let Some(index) = self.sidebar_tree.selected_position() {
            self.sidebar_scroll_handle.scroll_to_item(index);
        }
    }

    fn open_sidebar_file(&mut self, file: Option<usize>, cx: &mut Context<Self>) {
        if let Some(file) = file {
            self.select_file(file, cx);
        }
        self.reveal_sidebar_selection();
    }

    fn select_boundary(&mut self, end: bool, cx: &mut Context<Self>) {
        match self.pane {
            ViewerPane::Diff => {
                self.session.select_boundary(end);
                self.reveal_selected_row();
            }
            ViewerPane::Files => {
                let file = self
                    .sidebar_tree
                    .select_boundary(end)
                    .map(|entry| entry.file);
                self.open_sidebar_file(file.flatten(), cx);
            }
        }
        cx.notify();
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "the viewport row count is a small, non-negative UI measurement"
    )]
    fn page_items(&mut self, direction: isize, cx: &mut Context<Self>) {
        let rows = match self.pane {
            ViewerPane::Diff => {
                let height = f32::from(self.diff_list_state.viewport_bounds().size.height);
                (height / self.diff_row_height()).floor().max(1.0) as isize
            }
            ViewerPane::Files => self
                .sidebar_scroll_handle
                .bottom_item()
                .saturating_sub(self.sidebar_scroll_handle.top_item())
                .saturating_add(1)
                .max(1)
                .try_into()
                .unwrap_or(isize::MAX),
        };
        self.move_item(direction.saturating_mul(rows), cx);
    }

    fn next_item(&mut self, _: &NextItem, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::MoveSelection(1), window, cx);
    }

    fn previous_item(&mut self, _: &PreviousItem, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::MoveSelection(-1), window, cx);
    }

    fn first_item(&mut self, _: &FirstItem, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::First, window, cx);
    }

    fn last_item(&mut self, _: &LastItem, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::Last, window, cx);
    }

    fn page_up(&mut self, _: &PageUp, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::Page(-1), window, cx);
    }

    fn page_down(&mut self, _: &PageDown, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::Page(1), window, cx);
    }

    fn next_file(&mut self, _: &NextFile, window: &mut Window, cx: &mut Context<Self>) {
        self.move_file(1, window, cx);
    }

    fn previous_file(&mut self, _: &PreviousFile, window: &mut Window, cx: &mut Context<Self>) {
        self.move_file(-1, window, cx);
    }

    fn next_hunk(&mut self, _: &NextHunk, window: &mut Window, cx: &mut Context<Self>) {
        self.move_hunk(1, window, cx);
    }

    fn previous_hunk(&mut self, _: &PreviousHunk, window: &mut Window, cx: &mut Context<Self>) {
        self.move_hunk(-1, window, cx);
    }

    fn toggle_pane(&mut self, _: &TogglePane, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::ToggleFocus, window, cx);
    }

    fn focus_files(&mut self, _: &FocusFiles, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::Focus(ViewerPane::Files), window, cx);
    }

    fn focus_diff(&mut self, _: &FocusDiff, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::Focus(ViewerPane::Diff), window, cx);
    }

    fn select_old_side(&mut self, _: &SelectOldSide, window: &mut Window, cx: &mut Context<Self>) {
        if self.layout().is_split() {
            self.handle_command(DiffReviewCommand::SelectSide(DiffSide::Old), window, cx);
        }
    }

    fn select_new_side(&mut self, _: &SelectNewSide, window: &mut Window, cx: &mut Context<Self>) {
        if self.layout().is_split() {
            self.handle_command(DiffReviewCommand::SelectSide(DiffSide::New), window, cx);
        }
    }

    fn expand_or_open(&mut self, _: &ExpandOrOpen, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::OpenSelected, window, cx);
    }

    fn open_selected_entry(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.sidebar_tree.selected().cloned() else {
            return;
        };
        match entry.file {
            None => self.sidebar_tree.set_expanded(&entry.path, true),
            Some(index) => {
                self.select_file(index, cx);
                self.pane = ViewerPane::Diff;
            }
        }
        self.reveal_sidebar_selection();
        cx.notify();
    }

    fn collapse(&mut self, _: &Collapse, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::CollapseSelected, window, cx);
    }

    fn collapse_selected_entry(&mut self, cx: &mut Context<Self>) {
        if let Some(entry) = self.sidebar_tree.selected().cloned()
            && entry.file.is_none()
        {
            self.sidebar_tree.set_expanded(&entry.path, false);
            self.reveal_sidebar_selection();
            cx.notify();
        }
    }

    fn toggle_stage(&mut self, _: &ToggleStage, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::ToggleStage, window, cx);
    }

    fn stage_all(&mut self, _: &StageAll, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::StageAll, window, cx);
    }

    fn unstage_all(&mut self, _: &UnstageAll, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::UnstageAll, window, cx);
    }

    fn begin_commit_action(
        &mut self,
        _: &CommitChanges,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_command(DiffReviewCommand::BeginCommit, window, cx);
    }

    fn open_commit_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let editor = cx.new(|cx| {
            CommentEditor::with_placeholder(
                String::new(),
                self.theme.clone(),
                "Commit message…",
                window,
                cx,
            )
        });
        self.repository_editor_subscription = Some(cx.subscribe_in(
            &editor,
            window,
            |viewer, _editor, event: &CommentEditorEvent, window, cx| match event {
                CommentEditorEvent::Changed(_) => cx.notify(),
                CommentEditorEvent::Submit => viewer.finish_repository_commit(window, cx),
                CommentEditorEvent::Cancel => {
                    viewer.handle_command(ReviewCommand::Cancel, window, cx);
                }
            },
        ));
        self.repository_prompt = Some(RepositoryPrompt::Commit);
        self.repository_editor = Some(editor.clone());
        editor.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn begin_discard_action(
        &mut self,
        _: &DiscardChanges,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_command(DiffReviewCommand::BeginDiscard, window, cx);
    }

    fn open_discard_prompt(&mut self, cx: &mut Context<Self>) {
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
        cx.notify();
    }

    fn finish_repository_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = &self.repository_editor else {
            return;
        };
        let message = editor.read(cx).body().trim().to_owned();
        if message.is_empty() {
            self.repository_error = Some("Commit message cannot be empty".to_owned());
            cx.notify();
            return;
        }
        self.clear_repository_prompt();
        self.handle_command(
            DiffReviewCommand::RepositoryAction(RepositoryAction::Commit { message }),
            window,
            cx,
        );
        cx.notify();
    }

    fn confirm_discard(&mut self, _: &ConfirmDiscard, window: &mut Window, cx: &mut Context<Self>) {
        let Some(RepositoryPrompt::Discard { path, status }) = self.repository_prompt.clone()
        else {
            return;
        };
        self.clear_repository_prompt();
        self.handle_command(
            DiffReviewCommand::RepositoryAction(RepositoryAction::Discard { path, status }),
            window,
            cx,
        );
        cx.notify();
    }

    fn cancel_repository_prompt(
        &mut self,
        _: &CancelRepositoryPrompt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_command(ReviewCommand::Cancel, window, cx);
    }

    fn clear_repository_prompt(&mut self) {
        self.repository_prompt = None;
        self.repository_editor = None;
        self.repository_editor_subscription = None;
    }

    pub(crate) fn cycle_scope(
        &mut self,
        _: &CycleScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_command(DiffReviewCommand::CycleScope, window, cx);
    }

    fn cycle_view_mode(&mut self, _: &CycleViewMode, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::CycleViewMode, window, cx);
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
        self.handle_command(ReviewCommand::BeginComment, window, cx);
    }

    fn edit_comment_action(
        &mut self,
        _: &EditComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_command(ReviewCommand::EditComment, window, cx);
    }

    fn delete_comment_action(
        &mut self,
        _: &DeleteComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_command(ReviewCommand::DeleteComment, window, cx);
    }

    fn undo_comment(&mut self, _: &UndoComment, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(ReviewCommand::UndoComment, window, cx);
    }

    fn submit_comment(&mut self, _: &SubmitComment, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(ReviewCommand::SubmitComment, window, cx);
    }

    fn cancel_comment(&mut self, _: &CancelComment, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(ReviewCommand::Cancel, window, cx);
    }

    pub(crate) fn copy_review(
        &mut self,
        _: &CopyReview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_command(DiffReviewCommand::CopyReview, window, cx);
    }

    fn show_shortcuts(&mut self, _: &ShowShortcuts, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(ReviewCommand::ShowHelp, window, cx);
    }

    fn hide_shortcuts(&mut self, _: &HideShortcuts, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(ReviewCommand::Cancel, window, cx);
    }

    pub(crate) fn show_theme_picker(
        &mut self,
        _: &ShowThemePicker,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_command(ReviewCommand::OpenThemePicker, window, cx);
    }

    fn hide_theme_picker(
        &mut self,
        _: &HideThemePicker,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_command(ReviewCommand::Cancel, window, cx);
    }

    fn next_theme(&mut self, _: &NextTheme, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(ReviewCommand::MoveTheme(1), window, cx);
    }

    fn previous_theme(&mut self, _: &PreviousTheme, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(ReviewCommand::MoveTheme(-1), window, cx);
    }

    fn commit_theme(&mut self, _: &CommitTheme, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(ReviewCommand::CommitTheme, window, cx);
    }

    fn refresh(&mut self, _: &Refresh, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::Refresh, window, cx);
    }

    fn select_theme(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.theme_selection.as_ref().and_then(|selection| {
            selection
                .themes()
                .iter()
                .position(|choice| choice.theme.id().to_string() == id)
        });
        if let Some(index) = index {
            self.handle_command(ReviewCommand::SelectTheme(index), window, cx);
            self.handle_command(ReviewCommand::CommitTheme, window, cx);
        }
    }

    fn render_theme_picker(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.theme.id().to_string();
        let items = ReviewTheme::catalog().into_iter().map(|descriptor| {
            let id = descriptor.id.clone();
            ThemePickerItem::new(
                descriptor.id.clone(),
                descriptor.name,
                descriptor.is_dark,
                descriptor.id == current,
                cx.listener(move |viewer, _, window, cx| viewer.select_theme(&id, window, cx)),
            )
        });
        let viewport = window.viewport_size();
        ThemePicker::new(
            "theme-picker",
            self.ui_theme(),
            f32::from(viewport.width),
            f32::from(viewport.height),
        )
        .items(items)
    }

    pub(crate) fn submit_review(
        &mut self,
        _: &SubmitReview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_command(DiffReviewCommand::SubmitReview, window, cx);
    }

    pub(crate) fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(ReviewCommand::Cancel, window, cx);
    }

    fn render_repository_prompt(&self) -> impl IntoElement {
        let palette = &self.theme.diff;
        let prompt = self.repository_prompt.clone();
        let title = match &prompt {
            Some(RepositoryPrompt::Commit) => "Commit staged changes".to_owned(),
            Some(RepositoryPrompt::Discard { path, .. }) => format!("Discard changes to {path}?"),
            None => String::new(),
        };
        Modal::new("repository-prompt", title, self.ui_theme())
            .children(self.repository_editor.clone())
            .when(self.repository_editor.is_some(), |panel| {
                panel.child(
                    div()
                        .mt_2()
                        .text_color(color(palette.muted))
                        .child("Enter to commit · Esc to cancel"),
                )
            })
            .when(
                matches!(prompt, Some(RepositoryPrompt::Discard { .. })),
                |panel| {
                    panel
                        .child(
                            div()
                                .text_color(color(palette.deletion))
                                .child("This removes both staged and unstaged changes."),
                        )
                        .child(
                            div()
                                .mt_2()
                                .text_color(color(palette.muted))
                                .child("y to confirm · n or Esc to cancel"),
                        )
                },
            )
    }
}

impl EventEmitter<DiffViewerEvent> for DiffViewer {}
impl EventEmitter<ThemeChanged> for DiffViewer {}

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
    #[expect(
        clippy::too_many_lines,
        reason = "GPUI action and overlay wiring is declarative"
    )]
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport_width = f32::from(window.viewport_size().width);
        let preferred_width = self.preferred_sidebar_width.unwrap_or_else(|| {
            (viewport_width * DEFAULT_SIDEBAR_FRACTION).max(DEFAULT_MIN_SIDEBAR_WIDTH)
        });
        let sidebar_width = clamp_sidebar_width(preferred_width, viewport_width);
        if (self.sidebar_width - sidebar_width).abs() > f32::EPSILON {
            self.sidebar_width = sidebar_width;
            self.diff_list_state.remeasure();
        }
        let diff_width = viewport_width - sidebar_width - SIDEBAR_DIVIDER_WIDTH;
        let split_when_auto = diff_width >= self.options.auto_split_width;
        if self.session.split_when_auto() != split_when_auto {
            self.change_layout(|session| session.set_split_when_auto(split_when_auto));
        }
        self.follow_active_file();
        let palette = self.theme.diff.clone();
        let focus_handle = self
            .focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        if window.focused(cx).is_none()
            && self.comment_editor.is_none()
            && self.repository_editor.is_none()
        {
            focus_handle.focus(window, cx);
        }
        let mode = if self.theme_selection.is_some() {
            "themes"
        } else if self.shortcuts_open {
            "shortcuts"
        } else if matches!(
            self.repository_prompt,
            Some(RepositoryPrompt::Discard { .. })
        ) {
            "repository-discard"
        } else if self.repository_editor.is_some() {
            "repository-commit"
        } else if self.comment_editor.is_some() {
            "draft"
        } else {
            "browse"
        };
        let pane = match self.pane {
            ViewerPane::Files => "files",
            ViewerPane::Diff => "diff",
        };
        let layout = if self.layout().is_split() {
            "split"
        } else {
            "unified"
        };
        let mut key_context = KeyContext::default();
        key_context.add("DiffViewer");
        key_context.set("mode", mode);
        key_context.set("pane", pane);
        key_context.set("layout", layout);
        div()
            .debug_selector(|| "diff-viewer".to_owned())
            .key_context(key_context)
            .track_focus(&focus_handle)
            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                focus_handle.focus(window, cx);
            })
            .on_action(cx.listener(Self::next_item))
            .on_action(cx.listener(Self::previous_item))
            .on_action(cx.listener(Self::first_item))
            .on_action(cx.listener(Self::last_item))
            .on_action(cx.listener(Self::page_up))
            .on_action(cx.listener(Self::page_down))
            .on_action(cx.listener(Self::toggle_pane))
            .on_action(cx.listener(Self::focus_files))
            .on_action(cx.listener(Self::focus_diff))
            .on_action(cx.listener(Self::select_old_side))
            .on_action(cx.listener(Self::select_new_side))
            .on_action(cx.listener(Self::expand_or_open))
            .on_action(cx.listener(Self::collapse))
            .on_action(cx.listener(Self::next_file))
            .on_action(cx.listener(Self::previous_file))
            .on_action(cx.listener(Self::next_hunk))
            .on_action(cx.listener(Self::previous_hunk))
            .on_action(cx.listener(Self::cycle_scope))
            .on_action(cx.listener(Self::cycle_view_mode))
            .on_action(cx.listener(Self::expand_gap_action))
            .on_action(cx.listener(Self::activate_gap_action))
            .on_action(cx.listener(Self::expand_gap_all_action))
            .on_action(cx.listener(Self::toggle_full_file_action))
            .on_action(cx.listener(Self::toggle_source_view_action))
            .on_action(cx.listener(Self::increase_font_size))
            .on_action(cx.listener(Self::decrease_font_size))
            .on_action(cx.listener(Self::reset_font_size_action))
            .on_action(cx.listener(Self::toggle_stage))
            .on_action(cx.listener(Self::stage_all))
            .on_action(cx.listener(Self::unstage_all))
            .on_action(cx.listener(Self::begin_commit_action))
            .on_action(cx.listener(Self::begin_discard_action))
            .on_action(cx.listener(Self::confirm_discard))
            .on_action(cx.listener(Self::cancel_repository_prompt))
            .on_action(cx.listener(Self::add_comment_action))
            .on_action(cx.listener(Self::edit_comment_action))
            .on_action(cx.listener(Self::delete_comment_action))
            .on_action(cx.listener(Self::undo_comment))
            .on_action(cx.listener(Self::submit_comment))
            .on_action(cx.listener(Self::cancel_comment))
            .on_action(cx.listener(Self::copy_review))
            .on_action(cx.listener(Self::submit_review))
            .on_action(cx.listener(Self::show_shortcuts))
            .on_action(cx.listener(Self::hide_shortcuts))
            .on_action(cx.listener(Self::show_theme_picker))
            .on_action(cx.listener(Self::hide_theme_picker))
            .on_action(cx.listener(Self::next_theme))
            .on_action(cx.listener(Self::previous_theme))
            .on_action(cx.listener(Self::commit_theme))
            .on_action(cx.listener(Self::refresh))
            .on_action(cx.listener(Self::cancel))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(color(palette.background))
            .text_color(color(palette.foreground))
            .font_family(DEFAULT_FONT_FAMILY)
            .text_size(px(self.font_size))
            .child(
                div()
                    .id("diff-viewer-content")
                    .debug_selector(|| "diff-viewer-content".to_owned())
                    .flex_1()
                    .overflow_hidden()
                    .flex()
                    .on_drag_move::<SidebarResizeDrag>(cx.listener(Self::resize_sidebar))
                    .child(self.render_sidebar(cx))
                    .child(self.render_sidebar_resize_handle(cx))
                    .child(self.render_diff(window, cx)),
            )
            .child(self.render_review_bar(cx))
            .when(self.shortcuts_open, |viewer| {
                viewer.child(self.render_shortcuts())
            })
            .when(self.theme_selection.is_some(), |viewer| {
                viewer.child(self.render_theme_picker(window, cx))
            })
            .when(self.repository_prompt.is_some(), |viewer| {
                viewer.child(self.render_repository_prompt())
            })
            .when_some(
                self.repository_error().map(str::to_owned),
                |viewer, error| {
                    viewer.child(
                        div()
                            .absolute()
                            .bottom_4()
                            .left_4()
                            .right_4()
                            .child(Notification::error(error, self.ui_theme())),
                    )
                },
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clankerdiff_core::testing::DocumentBuilder;

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn sidebar_width_is_clamped_to_preserve_the_diff_pane() {
        assert_close(clamp_sidebar_width(100.0, 1_000.0), 180.0);
        assert_close(clamp_sidebar_width(400.0, 1_000.0), 400.0);
        assert_close(clamp_sidebar_width(900.0, 1_000.0), 680.0);
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
        assert_close(effective_diff_row_height(20.0, 13.0), 20.0);
        assert_close(effective_diff_row_height(20.0, 16.0), 23.0);
        assert_close(effective_diff_row_height(20.0, 20.0), 27.0);
    }

    #[test]
    fn deep_split_frames_parse_each_snapshot_document_once() {
        fn source_lines(suffix: &str) -> String {
            use std::fmt::Write;
            (1..=1_500).fold(String::new(), |mut source, i| {
                let _ = writeln!(source, "let value_{i} = {i}{suffix};");
                source
            })
        }
        let fixture = DocumentBuilder::new()
            .changed("src/large.rs", &source_lines(""), &source_lines(" + 1"))
            .build();
        let mut viewer = DiffViewer::new(fixture);
        viewer.session_mut().set_view_mode(ViewMode::Split);
        let rows: Vec<PresentedRow> = viewer.presentation().rows(1_200..1_224).to_vec();
        for _frame in 0..3 {
            for row in &rows {
                if row.kind != clankerdiff_core::RowKind::Code {
                    continue;
                }
                for cell in row.cells() {
                    let _ = viewer.highlight_cell(row, cell);
                }
            }
        }
        assert_eq!(
            viewer.highlight_stats().misses,
            2,
            "each complete side is parsed once across repeated frames"
        );
    }
}
