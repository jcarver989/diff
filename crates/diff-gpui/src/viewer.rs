#![allow(missing_docs)] // GPUI's `actions!` macro cannot attach per-action rustdoc.

use crate::{
    DEFAULT_FONT_FAMILY, DiffViewerEvent, ThemeChanged,
    comment_editor::{CommentEditor, CommentEditorEvent},
    sidebar::{SidebarResizeDrag, SidebarTree},
    style::color,
    ui::prelude::{Modal, Notification, ThemePicker, ThemePickerItem, UiTheme},
};
use diff_core::{
    DiffDocument, DiffPresentation, DiffSide, DiffSnapshot, FileStatus, Layout, LineAnchor,
    PresentedCell, PresentedRow, RepoPath, RepositoryAction, RevealAmount, Review, ReviewSession,
    SessionOptions, StageState, ViewMode,
};
use diff_syntax::{HighlightSpan, HighlightStats, LanguageHint, SyntaxHighlighter};
use diff_theme::DiffTheme;
use gpui::{
    App, Context, DragMoveEvent, Entity, EventEmitter, Focusable, KeyBinding, KeyContext,
    ListAlignment, ListState, ScrollHandle, Subscription, Window, actions, div, prelude::*, px,
};
use std::sync::Arc;

const MIN_SIDEBAR_WIDTH: f32 = 180.0;
const MAX_SIDEBAR_WIDTH: f32 = 600.0;
const MIN_DIFF_WIDTH: f32 = 320.0;
const SIDEBAR_DIVIDER_WIDTH: f32 = 1.0;
const MIN_FONT_SIZE: f32 = 10.0;
const MAX_FONT_SIZE: f32 = 24.0;
const FONT_SIZE_STEP: f32 = 1.0;
const DIFF_ROW_VERTICAL_SPACE: f32 = 7.0;
const SIDEBAR_ROW_VERTICAL_SPACE: f32 = 20.0;

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
        IncreaseFontSize,
        DecreaseFontSize,
        ResetFontSize,
        ToggleStage,
        StageAll,
        UnstageAll,
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
            font_size: 16.0,
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

/// The pane currently receiving browse-mode navigation commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerPane {
    /// The changed-files tree.
    Files,
    /// The selected file's diff.
    Diff,
}

/// Shared GPUI diff review view.
///
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent UI visibility, layout, and pending flags are not one state machine"
)]
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
    pub(crate) sidebar_selection: crate::sidebar::SidebarEntry,
    pub(crate) sidebar_scroll_handle: ScrollHandle,
    pub(crate) pane: ViewerPane,
    shortcuts_open: bool,
    theme_picker_open: bool,
    pub(crate) comment_target: Option<CommentTarget>,
    pub(crate) comment_editor: Option<Entity<CommentEditor>>,
    comment_editor_subscription: Option<Subscription>,
    repository_prompt: Option<RepositoryPrompt>,
    repository_editor: Option<Entity<CommentEditor>>,
    repository_editor_subscription: Option<Subscription>,
    repository_error: Option<String>,
    repository_pending: bool,
    focus_handle: Option<gpui::FocusHandle>,
}

impl DiffViewer {
    /// Creates a viewer with the default Sage theme and options.
    #[must_use]
    pub fn new(document: Arc<DiffDocument>) -> Self {
        Self::with_options(document, DiffTheme::default(), DiffViewerOptions::default())
    }

    /// Creates a viewer directly from an immutable native snapshot.
    #[must_use]
    pub fn from_snapshot(snapshot: DiffSnapshot) -> Self {
        Self::from_snapshot_with_options(
            snapshot,
            DiffTheme::default(),
            DiffViewerOptions::default(),
        )
    }

    #[must_use]
    pub fn from_snapshot_with_options(
        snapshot: DiffSnapshot,
        theme: DiffTheme,
        options: DiffViewerOptions,
    ) -> Self {
        let (document, _) = snapshot.clone().into_parts();
        let mut viewer = Self::with_options(document, theme, options);
        viewer.session = ReviewSession::from_snapshot(snapshot);
        viewer
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
            sidebar_selection: crate::sidebar::SidebarEntry::File(0),
            sidebar_scroll_handle: ScrollHandle::new(),
            pane: ViewerPane::Diff,
            shortcuts_open: false,
            theme_picker_open: false,
            comment_target: None,
            comment_editor: None,
            comment_editor_subscription: None,
            repository_prompt: None,
            repository_editor: None,
            repository_editor_subscription: None,
            repository_error: None,
            repository_pending: false,
            focus_handle: None,
        }
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
            KeyBinding::new("tab", TogglePane, Some(BROWSE)),
            KeyBinding::new("h", FocusFiles, Some(DIFF)),
            KeyBinding::new("left", SelectOldSide, Some(DIFF_SPLIT)),
            KeyBinding::new("left", FocusFiles, Some(DIFF_UNIFIED)),
            KeyBinding::new("right", SelectNewSide, Some(DIFF_SPLIT)),
            KeyBinding::new("h", Collapse, Some(FILES)),
            KeyBinding::new("left", Collapse, Some(FILES)),
            KeyBinding::new("l", ExpandOrOpen, Some(FILES)),
            KeyBinding::new("right", ExpandOrOpen, Some(FILES)),
            KeyBinding::new("enter", ExpandOrOpen, Some(FILES)),
            KeyBinding::new("space", ToggleStage, Some(FILES)),
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
            KeyBinding::new("o", ExpandGap, Some(DIFF)),
            KeyBinding::new("shift-o", ExpandGapAll, Some(DIFF)),
            KeyBinding::new("f", ToggleFullFile, Some(DIFF)),
            KeyBinding::new("enter", ActivateGap, Some(DIFF)),
            KeyBinding::new("v", CycleViewMode, Some(BROWSE)),
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
    pub fn set_snapshot(&mut self, snapshot: DiffSnapshot, cx: &mut Context<Self>) {
        let document = snapshot.document().clone();
        self.sidebar_tree.rebuild(&document);
        self.session.set_snapshot(snapshot);
        self.repository_pending = false;
        self.repository_error = None;
        self.diff_list_file = None;
        cx.notify();
    }

    pub fn set_document(&mut self, document: Arc<DiffDocument>, cx: &mut Context<Self>) {
        self.sidebar_tree.rebuild(&document);
        self.session.set_document(document);
        self.repository_pending = false;
        self.repository_error = None;
        self.sidebar_selection =
            crate::sidebar::SidebarEntry::File(self.session.selected_file().unwrap_or(0));
        if let Some(index) = self.selected_file() {
            let document = self.session.document().clone();
            self.sidebar_tree.expand_file(&document, index);
        }
        self.session.cancel_draft();
        self.clear_comment_editor();
        self.diff_list_file = None;
        cx.notify();
    }

    fn finish_layout_change(&mut self, changed: bool, cx: &mut Context<Self>) -> bool {
        if changed {
            self.diff_list_file = None;
            cx.notify();
        }
        changed
    }

    pub(crate) fn expand_selected_gap(&mut self, amount: RevealAmount, cx: &mut Context<Self>) {
        let selected = self.session.selected_row();
        let file = self.session.selected_file();
        let old_range = file.and_then(|file| self.session.presentation().file_range(file));
        let selected_gap =
            selected.is_some_and(|row| self.session.presentation().gap_info(row).is_some());
        if self.session.reveal_selected_gap(amount) {
            let new_range = file.and_then(|file| self.session.presentation().file_range(file));
            let can_splice = selected_gap
                && self.diff_list_file == file
                && old_range
                    .as_ref()
                    .is_some_and(|range| self.diff_list_state.item_count() == range.len());
            if let (true, Some(selected), Some(old_range), Some(new_range)) =
                (can_splice, selected, old_range, new_range)
            {
                let local = selected.saturating_sub(old_range.start);
                let replacement_count = new_range
                    .len()
                    .saturating_sub(old_range.len())
                    .saturating_add(1);
                self.diff_list_state
                    .splice(local..local.saturating_add(1), replacement_count);
            } else {
                self.diff_list_file = None;
            }
            cx.notify();
        }
    }

    pub(crate) fn toggle_full_file_projection(&mut self, cx: &mut Context<Self>) {
        if self.session.toggle_full_file() {
            self.diff_list_file = None;
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
        let changed = self.session.set_view_mode(mode);
        self.finish_layout_change(changed, cx);
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
        self.sidebar_selection = crate::sidebar::SidebarEntry::File(index);
        if self.session.select_file(index) {
            let document = self.session.document().clone();
            self.sidebar_tree.expand_file(&document, index);
            self.clear_comment_editor();
            cx.notify();
        }
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "GPUI click handlers construct owned entries"
    )]
    pub(crate) fn toggle_stage_entry(
        &mut self,
        entry: crate::sidebar::SidebarEntry,
        cx: &mut Context<Self>,
    ) {
        if self.repository_pending {
            return;
        }
        let state = SidebarTree::stage_state_for_entry(self.document(), &entry);
        let paths = SidebarTree::paths_for_entry(self.document(), &entry);
        if paths.is_empty() {
            return;
        }
        let action = if state == StageState::Staged {
            RepositoryAction::UnstagePaths(paths)
        } else {
            RepositoryAction::StagePaths(paths)
        };
        cx.emit(DiffViewerEvent::RepositoryAction(action));
    }

    pub(crate) fn toggle_directory(&mut self, path: &str, cx: &mut Context<Self>) {
        self.pane = ViewerPane::Files;
        self.sidebar_selection = crate::sidebar::SidebarEntry::Directory(path.to_owned());
        self.sidebar_tree.toggle(path);
        cx.notify();
    }

    pub(crate) fn select_diff_cell(
        &mut self,
        row_index: usize,
        side: DiffSide,
        cx: &mut Context<Self>,
    ) {
        if self.session.select_row(row_index) && self.session.set_selected_side(side) {
            self.pane = ViewerPane::Diff;
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
        let previous_row = self.comment_target.as_ref().map(|target| target.row_index);
        if !self.session.select_row(row_index) || !self.session.set_selected_side(side) {
            return;
        }
        if !self.session.begin_draft(editing) {
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
        self.pane = ViewerPane::Diff;
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

    fn expand_gap_action(&mut self, _: &ExpandGap, _: &mut Window, cx: &mut Context<Self>) {
        self.expand_selected_gap(RevealAmount::Step, cx);
    }

    fn activate_gap_action(&mut self, _: &ActivateGap, _: &mut Window, cx: &mut Context<Self>) {
        let selected_gap = self
            .session
            .selected_row()
            .is_some_and(|row| self.session.presentation().gap_info(row).is_some());
        if selected_gap {
            self.expand_selected_gap(RevealAmount::Step, cx);
        }
    }

    fn expand_gap_all_action(&mut self, _: &ExpandGapAll, _: &mut Window, cx: &mut Context<Self>) {
        self.expand_selected_gap(RevealAmount::All, cx);
    }

    fn toggle_full_file_action(
        &mut self,
        _: &ToggleFullFile,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_full_file_projection(cx);
    }

    pub(crate) fn highlight_cell(
        &mut self,
        row: &PresentedRow,
        cell: &PresentedCell,
    ) -> Arc<[HighlightSpan]> {
        let presentation = self.session.presentation();
        let mut syntax = self.highlighter.with_theme(&self.theme);
        if let (Some(source), Some(path), Some(line)) = (
            presentation.source_document(row, cell),
            presentation.source_path(row, cell),
            cell.line_number().and_then(|line| line.checked_sub(1)),
        ) {
            return syntax
                .highlight_document(
                    source.sequence_id(),
                    LanguageHint::Path(path),
                    source.text(),
                )
                .line_shared(line)
                .unwrap_or_else(diff_syntax::empty_spans);
        }
        if let Some(sequence) = presentation.hunk_sequence(row, cell) {
            return syntax
                .highlight_document_lines(
                    sequence.id,
                    LanguageHint::Path(sequence.path),
                    sequence.lines(),
                )
                .line_shared(sequence.target_line)
                .unwrap_or_else(diff_syntax::empty_spans);
        }
        syntax.highlight_source(LanguageHint::Path(presentation.row_path(row)), &cell.text)
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
            self.reveal_selected_row();
            cx.notify();
        }
    }

    fn reveal_selected_row(&self) {
        let Some(selected) = self.session.selected_row() else {
            return;
        };
        let Some(range) = self.session.selected_file_range() else {
            return;
        };
        if range.contains(&selected) {
            self.diff_list_state
                .scroll_to_reveal_item(selected - range.start);
        }
    }

    fn move_item(&mut self, delta: isize, cx: &mut Context<Self>) {
        match self.pane {
            ViewerPane::Diff => {
                self.session.move_row(delta);
                self.reveal_selected_row();
            }
            ViewerPane::Files => {
                if let Some(selection) = self
                    .sidebar_tree
                    .offset_entry(&self.sidebar_selection, delta)
                {
                    self.sidebar_selection = selection.clone();
                    if let crate::sidebar::SidebarEntry::File(index) = selection {
                        self.select_file(index, cx);
                    }
                    self.reveal_sidebar_selection();
                }
            }
        }
        cx.notify();
    }

    fn reveal_sidebar_selection(&self) {
        if let Some(index) = self.sidebar_tree.position_of(&self.sidebar_selection) {
            self.sidebar_scroll_handle.scroll_to_item(index);
        }
    }

    fn select_boundary(&mut self, end: bool, cx: &mut Context<Self>) {
        match self.pane {
            ViewerPane::Diff => {
                self.session.select_boundary(end);
                self.reveal_selected_row();
            }
            ViewerPane::Files => {
                if let Some(selection) = self.sidebar_tree.boundary_entry(end) {
                    self.sidebar_selection = selection.clone();
                    if let crate::sidebar::SidebarEntry::File(index) = selection {
                        self.select_file(index, cx);
                    }
                    self.reveal_sidebar_selection();
                }
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
        self.move_item(direction * rows, cx);
    }

    fn next_item(&mut self, _: &NextItem, _: &mut Window, cx: &mut Context<Self>) {
        self.move_item(1, cx);
    }

    fn previous_item(&mut self, _: &PreviousItem, _: &mut Window, cx: &mut Context<Self>) {
        self.move_item(-1, cx);
    }

    fn first_item(&mut self, _: &FirstItem, _: &mut Window, cx: &mut Context<Self>) {
        self.select_boundary(false, cx);
    }

    fn last_item(&mut self, _: &LastItem, _: &mut Window, cx: &mut Context<Self>) {
        self.select_boundary(true, cx);
    }

    fn page_up(&mut self, _: &PageUp, _: &mut Window, cx: &mut Context<Self>) {
        self.page_items(-1, cx);
    }

    fn page_down(&mut self, _: &PageDown, _: &mut Window, cx: &mut Context<Self>) {
        self.page_items(1, cx);
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

    fn toggle_pane(&mut self, _: &TogglePane, _: &mut Window, cx: &mut Context<Self>) {
        self.pane = match self.pane {
            ViewerPane::Files => ViewerPane::Diff,
            ViewerPane::Diff => ViewerPane::Files,
        };
        cx.notify();
    }

    fn focus_files(&mut self, _: &FocusFiles, _: &mut Window, cx: &mut Context<Self>) {
        self.pane = ViewerPane::Files;
        cx.notify();
    }

    fn focus_diff(&mut self, _: &FocusDiff, _: &mut Window, cx: &mut Context<Self>) {
        self.pane = ViewerPane::Diff;
        cx.notify();
    }

    fn select_old_side(&mut self, _: &SelectOldSide, _: &mut Window, cx: &mut Context<Self>) {
        if self.layout().is_split() && self.session.set_selected_side(DiffSide::Old) {
            cx.notify();
        }
    }

    fn select_new_side(&mut self, _: &SelectNewSide, _: &mut Window, cx: &mut Context<Self>) {
        if self.layout().is_split() && self.session.set_selected_side(DiffSide::New) {
            cx.notify();
        }
    }

    fn expand_or_open(&mut self, _: &ExpandOrOpen, _: &mut Window, cx: &mut Context<Self>) {
        match self.sidebar_selection.clone() {
            crate::sidebar::SidebarEntry::Directory(path) => self.sidebar_tree.expand(&path),
            crate::sidebar::SidebarEntry::File(index) => {
                self.select_file(index, cx);
                self.pane = ViewerPane::Diff;
            }
        }
        self.reveal_sidebar_selection();
        cx.notify();
    }

    fn collapse(&mut self, _: &Collapse, _: &mut Window, cx: &mut Context<Self>) {
        if let crate::sidebar::SidebarEntry::Directory(path) = self.sidebar_selection.clone() {
            self.sidebar_tree.collapse(&path);
            self.reveal_sidebar_selection();
            cx.notify();
        }
    }

    fn toggle_stage(&mut self, _: &ToggleStage, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_stage_entry(self.sidebar_selection.clone(), cx);
    }

    fn stage_all(&mut self, _: &StageAll, _: &mut Window, cx: &mut Context<Self>) {
        if !self.repository_pending {
            cx.emit(DiffViewerEvent::RepositoryAction(
                RepositoryAction::StageAll,
            ));
        }
    }

    fn unstage_all(&mut self, _: &UnstageAll, _: &mut Window, cx: &mut Context<Self>) {
        if !self.repository_pending {
            cx.emit(DiffViewerEvent::RepositoryAction(
                RepositoryAction::UnstageAll,
            ));
        }
    }

    fn begin_commit_action(
        &mut self,
        _: &CommitChanges,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.repository_pending {
            return;
        }
        let editor = cx.new(|cx| {
            CommentEditor::with_placeholder(
                String::new(),
                self.theme.clone(),
                "Commit message…",
                cx,
            )
        });
        self.repository_editor_subscription = Some(cx.subscribe(
            &editor,
            |viewer, _editor, event: &CommentEditorEvent, cx| match event {
                CommentEditorEvent::Changed(_) => cx.notify(),
                CommentEditorEvent::Submit => viewer.finish_repository_commit(cx),
                CommentEditorEvent::Cancel => {
                    viewer.clear_repository_prompt();
                    cx.notify();
                }
            },
        ));
        self.repository_prompt = Some(RepositoryPrompt::Commit);
        self.repository_editor = Some(editor.clone());
        editor.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn begin_discard_action(&mut self, _: &DiscardChanges, _: &mut Window, cx: &mut Context<Self>) {
        if self.repository_pending {
            return;
        }
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

    fn finish_repository_commit(&mut self, cx: &mut Context<Self>) {
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
        cx.emit(DiffViewerEvent::RepositoryAction(
            RepositoryAction::Commit { message },
        ));
        cx.notify();
    }

    fn confirm_discard(&mut self, _: &ConfirmDiscard, _: &mut Window, cx: &mut Context<Self>) {
        let Some(RepositoryPrompt::Discard { path, status }) = self.repository_prompt.clone()
        else {
            return;
        };
        self.clear_repository_prompt();
        cx.emit(DiffViewerEvent::RepositoryAction(
            RepositoryAction::Discard { path, status },
        ));
        cx.notify();
    }

    fn cancel_repository_prompt(
        &mut self,
        _: &CancelRepositoryPrompt,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_repository_prompt();
        cx.notify();
    }

    fn clear_repository_prompt(&mut self) {
        self.repository_prompt = None;
        self.repository_editor = None;
        self.repository_editor_subscription = None;
    }

    fn cycle_view_mode(&mut self, _: &CycleViewMode, _: &mut Window, cx: &mut Context<Self>) {
        let changed = self.session.cycle_view_mode();
        self.finish_layout_change(changed, cx);
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

    fn edit_comment_action(
        &mut self,
        _: &EditComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editing) = self.session.comment_id_at_selection() else {
            return;
        };
        let Some(row_index) = self.session.selected_row() else {
            return;
        };
        self.open_comment_editor(
            row_index,
            self.session.selected_side(),
            Some(editing),
            window,
            cx,
        );
    }

    fn delete_comment_action(&mut self, _: &DeleteComment, _: &mut Window, cx: &mut Context<Self>) {
        if self.session.delete_comment_at_selection() {
            self.diff_list_state.remeasure();
            cx.notify();
        }
    }

    fn undo_comment(&mut self, _: &UndoComment, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.session.last_comment_id()
            && self.session.review_mut().remove_comment(id).is_some()
        {
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

    fn show_shortcuts(&mut self, _: &ShowShortcuts, _: &mut Window, cx: &mut Context<Self>) {
        self.shortcuts_open = true;
        cx.notify();
    }

    fn hide_shortcuts(&mut self, _: &HideShortcuts, _: &mut Window, cx: &mut Context<Self>) {
        self.shortcuts_open = false;
        cx.notify();
    }

    pub(crate) fn show_theme_picker(
        &mut self,
        _: &ShowThemePicker,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.theme_picker_open = true;
        cx.notify();
    }

    fn hide_theme_picker(&mut self, _: &HideThemePicker, _: &mut Window, cx: &mut Context<Self>) {
        self.theme_picker_open = false;
        cx.notify();
    }

    fn select_theme(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Ok(theme) = DiffTheme::builtin(id) {
            self.theme_picker_open = false;
            self.set_theme(theme, cx);
            cx.emit(ThemeChanged { id: id.to_owned() });
        }
    }

    fn render_theme_picker(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.theme.id().to_string();
        let items = DiffTheme::catalog().into_iter().map(|descriptor| {
            let id = descriptor.id.clone();
            ThemePickerItem::new(
                descriptor.id.clone(),
                descriptor.name,
                descriptor.is_dark,
                descriptor.id == current,
                cx.listener(move |viewer, _, _, cx| viewer.select_theme(&id, cx)),
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

    fn render_repository_prompt(&self) -> impl IntoElement {
        let palette = self.theme.palette();
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
        let sidebar_width = clamp_sidebar_width(self.sidebar_width, viewport_width);
        if (self.sidebar_width - sidebar_width).abs() > f32::EPSILON {
            self.sidebar_width = sidebar_width;
            self.diff_list_state.remeasure();
        }
        let diff_width = viewport_width - sidebar_width - SIDEBAR_DIVIDER_WIDTH;
        if self
            .session
            .set_split_when_auto(diff_width >= self.options.auto_split_width)
        {
            self.diff_list_file = None;
        }
        let palette = self.theme.palette().clone();
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
        let mode = if self.theme_picker_open {
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
            .on_action(cx.listener(Self::cycle_view_mode))
            .on_action(cx.listener(Self::expand_gap_action))
            .on_action(cx.listener(Self::activate_gap_action))
            .on_action(cx.listener(Self::expand_gap_all_action))
            .on_action(cx.listener(Self::toggle_full_file_action))
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
            .when(self.theme_picker_open, |viewer| {
                viewer.child(self.render_theme_picker(window, cx))
            })
            .when(self.repository_prompt.is_some(), |viewer| {
                viewer.child(self.render_repository_prompt())
            })
            .when_some(self.repository_error.clone(), |viewer, error| {
                viewer.child(
                    div()
                        .absolute()
                        .bottom_4()
                        .left_4()
                        .right_4()
                        .child(Notification::error(error, self.ui_theme())),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diff_core::testing::DocumentBuilder;

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
            .build_fixture();
        let mut viewer = DiffViewer::from_snapshot(fixture.snapshot());
        viewer.session_mut().set_view_mode(ViewMode::Split);
        let rows: Vec<PresentedRow> = viewer.presentation().rows(1_200..1_224).to_vec();
        for _frame in 0..3 {
            for row in &rows {
                if row.kind != diff_core::RowKind::Code {
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
