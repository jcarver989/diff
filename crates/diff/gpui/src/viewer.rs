#![allow(missing_docs)] // GPUI's `actions!` macro cannot attach per-action rustdoc.

mod commands;
mod overlay;

use overlay::RepositoryPrompt;
pub(crate) use overlay::{Overlay, PickerOverlay};

use crate::{
    DiffViewerEvent, ThemeChanged, ViewModeChanged,
    comment_editor::{CommentEditor, CommentEditorEvent, init_input},
    style::color,
    ui::{
        prelude::{
            Modal, Notification, SearchPicker, ThemePicker, ThemePickerEvent, ThemePickerOverlay, Typography,
            TypographyExt, UI_FONT, UiTheme, rem,
        },
        tokens::{BASE_SIZE, CODE_ROW_HEIGHT},
    },
};
use clankerdiff_core::{Chord, DiffReviewCommand, Key, KeyScope, ReviewCapabilities, ReviewCommand, diff_shortcuts};
use clankerdiff_core::{
    DiffDocument, DiffPresentation, DiffScope, DiffSide, FileTree, InteractionPhase, Layout, LineAnchor, PresentedCell,
    PresentedRow, RepositoryAction, RevealAmount, Review, ReviewSession, ViewMode, ViewportAnchor,
};
use clankerdiff_symbols::{NameLookup, SymbolCache, SymbolIndex};
use clankerdiff_syntax::{HighlightSpan, HighlightStats, LanguageHint, SyntaxHighlighter, empty_spans};
use clankerdiff_ui_core::{ReviewTheme, ThemeChoice};
use gpui::{
    Action, App, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding, KeyContext, ListAlignment,
    ListOffset, ListState, Pixels, Rems, ScrollStrategy, Subscription, Task, UniformListScrollHandle, Window, actions,
    div, prelude::*, px, rems,
};
use std::sync::Arc;

pub(crate) const MIN_SIDEBAR_WIDTH: Rems = rem(180.0);
pub(crate) const MAX_SIDEBAR_WIDTH: Rems = rem(960.0);
const DEFAULT_SIDEBAR_FRACTION: f32 = 0.25;
const DEFAULT_MIN_SIDEBAR_WIDTH: Rems = rem(320.0);
const MIN_DIFF_WIDTH: Rems = rem(320.0);

actions!(diff_viewer, [SubmitComment, CancelComment, ConfirmDiscard, CancelRepositoryPrompt]);

#[derive(Debug, Clone, PartialEq, Action)]
#[action(namespace = diff_viewer, no_json)]
pub struct RunCommand(pub DiffReviewCommand);

/// Renderer-specific sizing and virtualization settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffViewerOptions {
    /// Initial sidebar width, or None for responsive sizing.
    pub sidebar_width: Option<Rems>,
    /// Diff-pane width at which automatic mode switches to split layout.
    pub auto_split_width: Rems,
    /// Maximum number of syntax-highlight cache entries.
    pub highlight_cache_capacity: usize,
    pub themes: bool,
    pub review_bar: bool,
    pub view_mode: ViewMode,
}

impl Default for DiffViewerOptions {
    fn default() -> Self {
        Self {
            sidebar_width: None,
            auto_split_width: rem(900.0),
            highlight_cache_capacity: 512,
            themes: true,
            review_bar: true,
            view_mode: ViewMode::Auto,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommentTarget {
    pub(crate) row_index: usize,
    pub(crate) side: DiffSide,
}

pub use clankerdiff_core::FocusPane as ViewerPane;

struct SidebarLayout {
    width: Rems,
    preferred_width: Option<Rems>,
    collapsed: bool,
}

struct DiffViewportBookmark {
    offset: ListOffset,
    anchor: Option<ViewportAnchor>,
}

/// Shared GPUI diff review view.
///
pub struct DiffViewer {
    pub(crate) session: ReviewSession,
    pub(crate) capabilities: ReviewCapabilities,
    pub(crate) overlay: Option<Overlay>,
    pub(crate) theme: ReviewTheme,
    pub(crate) highlighter: SyntaxHighlighter,
    pub(crate) options: DiffViewerOptions,
    sidebar: SidebarLayout,
    rem_size: Pixels,
    pub(crate) diff_list_state: ListState,
    diff_list_revision: Option<u64>,
    followed_file: Option<usize>,
    diff_list_split: bool,
    diff_viewport: Option<DiffViewportBookmark>,
    pub(crate) sidebar_tree: FileTree,
    pub(crate) sidebar_scroll_handle: UniformListScrollHandle,
    pub(crate) pane: ViewerPane,
    pub(crate) comment_target: Option<CommentTarget>,
    pub(crate) comment_editor: Option<Entity<CommentEditor>>,
    comment_editor_subscription: Option<Subscription>,
    comment_editor_focus: Option<FocusHandle>,
    repository_error: Option<String>,
    background_error: Option<String>,
    repository_pending: bool,
    focus_handle: Option<gpui::FocusHandle>,
    pub(crate) links_visible: bool,
    symbols: SymbolCache,
    pub(crate) names: NameLookup,
    pub(crate) card_timer: Option<Task<()>>,
}

impl DiffViewer {
    /// Creates a viewer with the default Sage theme and options.
    #[must_use]
    pub fn new(document: Arc<DiffDocument>) -> Self {
        Self::with_options(document, ReviewTheme::default(), DiffViewerOptions::default())
    }

    /// Creates a viewer with explicit theme and adapter options.
    #[must_use]
    pub fn with_options(document: Arc<DiffDocument>, theme: ReviewTheme, options: DiffViewerOptions) -> Self {
        let mut sidebar_tree = FileTree::new(&document);
        let sidebar_width = clamp_rems(
            options.sidebar_width.unwrap_or(DEFAULT_MIN_SIDEBAR_WIDTH),
            MIN_SIDEBAR_WIDTH,
            MAX_SIDEBAR_WIDTH,
        );
        let rem_size = px(BASE_SIZE);
        let row_height = rem(CODE_ROW_HEIGHT).to_pixels(rem_size);
        let mut session = ReviewSession::new(document);
        session.set_view_mode(options.view_mode);
        sidebar_tree.reveal_file(session.selected_file().unwrap_or(0));
        let mut viewer = Self {
            session,
            capabilities: ReviewCapabilities::default(),
            overlay: None,
            theme,
            highlighter: SyntaxHighlighter::new(options.highlight_cache_capacity),
            options,
            sidebar: SidebarLayout { width: sidebar_width, preferred_width: options.sidebar_width, collapsed: false },
            rem_size,
            diff_list_state: ListState::new(0, ListAlignment::Top, row_height * 8.0),
            diff_list_revision: None,
            followed_file: None,
            diff_list_split: false,
            diff_viewport: None,
            sidebar_tree,
            sidebar_scroll_handle: UniformListScrollHandle::new(),
            pane: ViewerPane::Diff,
            comment_target: None,
            comment_editor: None,
            comment_editor_subscription: None,
            comment_editor_focus: None,
            repository_error: None,
            background_error: None,
            repository_pending: false,
            focus_handle: None,
            links_visible: false,
            symbols: SymbolCache::default(),
            names: NameLookup::default(),
            card_timer: None,
        };
        viewer.sync_diff_list();
        viewer
    }

    pub fn bind_keys(cx: &mut App) {
        const DRAFT: &str = "DiffViewer && mode == draft";
        const DISCARD: &str = "DiffViewer && mode == repository-discard";
        let shortcuts = diff_shortcuts().into_iter().map(|(chord, scope, entry)| {
            KeyBinding::new(&keystroke(chord), RunCommand(entry.command.clone()), Some(&key_context(chord, scope)))
        });
        cx.bind_keys(shortcuts.chain([
            KeyBinding::new("cmd-enter", SubmitComment, Some(DRAFT)),
            KeyBinding::new("ctrl-enter", SubmitComment, Some(DRAFT)),
            KeyBinding::new("escape", CancelComment, Some(DRAFT)),
            KeyBinding::new("y", ConfirmDiscard, Some(DISCARD)),
            KeyBinding::new("n", CancelRepositoryPrompt, Some(DISCARD)),
            KeyBinding::new("escape", CancelRepositoryPrompt, Some(DISCARD)),
        ]));
        SearchPicker::bind_keys(cx);
    }

    #[must_use]
    pub const fn scope(&self) -> DiffScope {
        self.session.scope()
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

    #[must_use]
    pub const fn commands_open(&self) -> bool {
        matches!(self.overlay, Some(Overlay::Commands(_)))
    }

    /// Returns whether the theme picker is open.
    #[must_use]
    pub const fn theme_picker_open(&self) -> bool {
        matches!(self.overlay, Some(Overlay::Themes(_)))
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
    pub const fn sidebar_width(&self) -> Rems {
        self.sidebar.width
    }

    #[must_use]
    pub const fn sidebar_collapsed(&self) -> bool {
        self.sidebar.collapsed
    }

    pub fn set_sidebar_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        if self.sidebar.collapsed == collapsed {
            return;
        }
        self.sidebar.collapsed = collapsed;
        if collapsed {
            self.pane = ViewerPane::Diff;
        }
        self.diff_list_state.remeasure();
        cx.notify();
    }

    /// Changes the changed-files sidebar width.
    pub fn set_sidebar_width(&mut self, width: Rems, cx: &mut Context<Self>) {
        self.update_sidebar_width(clamp_rems(width, MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH), cx);
    }

    /// Returns highlighter cache/work counters.
    #[must_use]
    pub const fn highlight_stats(&self) -> HighlightStats {
        self.highlighter.stats()
    }

    pub fn set_document(&mut self, document: Arc<DiffDocument>, cx: &mut Context<Self>) {
        if self.session.commits() != document.commits.as_ref() {
            self.close_overlay(InteractionPhase::CommitPicker);
        }
        self.card_timer = None;
        self.sidebar_tree.rebuild(&document);
        self.change_layout(|session| {
            session.set_document(document);
            true
        });
        self.update_names(cx, |names, session, _, enabled| names.document_changed(session, enabled));
        if self.sidebar_tree.selected().is_none() {
            self.sidebar_tree.reveal_file(self.session.selected_file().unwrap_or(0));
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

    pub(crate) fn change_layout(&mut self, change: impl FnOnce(&mut ReviewSession) -> bool) -> bool {
        let bookmark = self.diff_viewport_bookmark();
        let changed = change(&mut self.session);
        if self.sync_diff_list() {
            self.restore_diff_viewport(bookmark);
        }
        changed
    }

    pub(crate) fn change_view_mode(
        &mut self,
        change: impl FnOnce(&mut ReviewSession) -> bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.change_layout(change) {
            return false;
        }
        cx.emit(ViewModeChanged { mode: self.session.view_mode() });
        true
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
            self.close_overlay(InteractionPhase::RepositoryPrompt);
        }
        cx.notify();
    }

    /// Shows a repository error without replacing the current document.
    pub fn set_repository_error(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.repository_pending = false;
        self.repository_error = Some(message.into());
        self.close_overlay(InteractionPhase::RepositoryPrompt);
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
        self.repository_error.as_deref().or(self.background_error.as_deref())
    }

    pub fn set_theme(&mut self, theme: ReviewTheme, cx: &mut Context<Self>) {
        self.close_overlay(InteractionPhase::ThemePicker);
        self.apply_theme(theme, cx);
    }

    fn open_theme_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        init_input(cx);
        let (theme, ui) = (self.theme.clone(), self.ui_theme());
        let overlay =
            ThemePickerOverlay::open(&theme, ThemeChoice::catalog(), ui, window, cx, |viewer, event, _, cx| {
                viewer.on_theme_picker_event(event, cx);
            });
        self.overlay = Some(Overlay::Themes(overlay));
    }

    fn on_theme_picker_event(&mut self, event: &ThemePickerEvent, cx: &mut Context<Self>) {
        match event {
            ThemePickerEvent::Preview(theme) => self.apply_theme(theme.clone(), cx),
            ThemePickerEvent::Commit(theme) => {
                self.overlay = None;
                self.apply_theme(theme.clone(), cx);
                cx.emit(ThemeChanged { id: theme.id().to_string() });
            }
            ThemePickerEvent::Cancel(theme) => {
                self.overlay = None;
                self.apply_theme(theme.clone(), cx);
            }
        }
    }

    const fn theme_picker_overlay(&self) -> Option<&ThemePickerOverlay> {
        match &self.overlay {
            Some(Overlay::Themes(overlay)) => Some(overlay),
            _ => None,
        }
    }

    fn update_theme_picker(
        &self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut ThemePicker, &mut Context<ThemePicker>),
    ) -> bool {
        let Some(Overlay::Themes(overlay)) = &self.overlay else {
            return false;
        };
        overlay.picker().update(cx, update);
        true
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
        let id = self.session.review_mut().add_comment_with_context(anchor, line_text, body);
        self.diff_list_state.remeasure();
        cx.notify();
        id
    }

    /// Edits a queued comment.
    pub fn edit_comment(&mut self, id: u64, body: impl Into<String>, cx: &mut Context<Self>) -> bool {
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
        if self.session.draft().is_some_and(|draft| draft.editing() == Some(id)) {
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

    pub(crate) fn diff_row_height(&self) -> Pixels {
        rem(CODE_ROW_HEIGHT).to_pixels(self.rem_size)
    }

    pub(crate) fn rem_size(&self) -> Pixels {
        self.rem_size
    }

    pub(crate) fn reset_sidebar_width(&mut self, cx: &mut Context<Self>) {
        self.sidebar.preferred_width = self.options.sidebar_width;
        cx.notify();
    }

    pub(crate) fn resize_sidebar(&mut self, width: Rems, window: &Window, cx: &mut Context<Self>) {
        let available = rems(window.viewport_size().width / window.rem_size());
        self.update_sidebar_width(clamp_sidebar_width(width, available), cx);
    }

    fn update_sidebar_width(&mut self, width: Rems, cx: &mut Context<Self>) {
        self.sidebar.preferred_width = Some(width);
        if self.sidebar.width == width {
            return;
        }
        self.sidebar.width = width;
        self.diff_list_state.remeasure();
        cx.notify();
    }

    #[must_use]
    pub fn active_file(&self) -> Option<usize> {
        let range = self.session.visible_range();
        let row = range.start + self.diff_list_state.logical_scroll_top().item_ix;
        Some(self.presentation().row(row.min(range.end.checked_sub(1)?))?.file_index)
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
            self.diff_list_state.reset_with_uniform_height(self.session.visible_range().len(), self.diff_row_height());
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
        DiffViewportBookmark { offset, anchor: ViewportAnchor::capture(self.presentation(), row) }
    }

    fn restore_diff_viewport(&self, bookmark: DiffViewportBookmark) {
        let range = self.session.visible_range();
        self.diff_list_state.scroll_to(
            match bookmark.anchor.and_then(|anchor| anchor.resolve(self.presentation(), range.clone())) {
                Some(row) => ListOffset { item_ix: row - range.start, ..bookmark.offset },
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
            if let Some(item_ix) = self.presentation().file_range(index).and_then(|range| self.list_item(range.start)) {
                self.diff_list_state.scroll_to(ListOffset { item_ix, offset_in_item: px(0.0) });
            }
            cx.notify();
        }
    }

    pub(crate) fn toggle_stage_path(&mut self, path: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.repository_pending {
            return;
        }
        let Some(action) = self.sidebar_tree.toggle_stage_action(self.document(), path) else {
            return;
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
        let command = if editing.is_some() { ReviewCommand::EditComment } else { ReviewCommand::BeginComment };
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
        let body = self.session.draft().map_or_else(String::new, |draft| draft.body().to_owned());
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
        if let Some(item) = self.list_item(row).filter(|&item| item < self.diff_list_state.item_count()) {
            let offset = self.diff_list_state.logical_scroll_top();
            self.diff_list_state.splice_focusable(item..item + 1, [focus]);
            self.diff_list_state.scroll_to(offset);
        }
    }

    pub(crate) fn remeasure_row(&self, row_index: Option<usize>) {
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

    pub(crate) fn symbols(&mut self) -> Arc<SymbolIndex> {
        self.symbols.index(self.session.presentation().document())
    }

    pub(crate) fn highlight_cell(&mut self, row: &PresentedRow, cell: &PresentedCell) -> Arc<[HighlightSpan]> {
        let source = self.session.presentation().cell_context(row, cell);
        self.highlighter
            .with_theme(&self.theme.syntax)
            .highlight_document(source.id, LanguageHint::Path(source.path), || source.text())
            .ok()
            .and_then(|highlights| highlights.line_shared(source.target_line))
            .unwrap_or_else(empty_spans)
    }

    pub(crate) fn reveal_selected_row(&self) {
        let Some(item) = self.session.selected_row().and_then(|row| self.list_item(row)) else {
            return;
        };
        let divider_height = self.file_divider_height();
        let under_divider = item <= self.diff_list_state.logical_scroll_top().item_ix
            || self
                .diff_list_state
                .bounds_for_item(item)
                .is_some_and(|bounds| bounds.top() < self.diff_list_state.viewport_bounds().top() + divider_height);
        if under_divider {
            self.diff_list_state.scroll_to(ListOffset { item_ix: item, offset_in_item: px(0.0) });
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
                let file = self.sidebar_tree.move_selection(delta).map(|entry| entry.file);
                self.open_sidebar_file(file.flatten(), cx);
            }
        }
        cx.notify();
    }

    fn reveal_sidebar_selection(&self) {
        if let Some(index) = self.sidebar_tree.selected_position() {
            self.sidebar_scroll_handle.scroll_to_item(index, ScrollStrategy::Nearest);
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
                let file = self.sidebar_tree.select_boundary(end).map(|entry| entry.file);
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
                let height = self.diff_list_state.viewport_bounds().size.height;
                (height / self.diff_row_height()).floor().max(1.0) as isize
            }
            ViewerPane::Files => self.sidebar_visible_rows().len().max(1).try_into().unwrap_or(isize::MAX),
        };
        self.move_item(direction.saturating_mul(rows), cx);
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

    fn collapse_selected_entry(&mut self, cx: &mut Context<Self>) {
        if let Some(entry) = self.sidebar_tree.selected().cloned()
            && entry.file.is_none()
        {
            self.sidebar_tree.set_expanded(&entry.path, false);
            self.reveal_sidebar_selection();
            cx.notify();
        }
    }

    fn open_commit_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let editor = cx.new(|cx| {
            CommentEditor::with_placeholder(String::new(), self.theme.clone(), "Commit message…", window, cx)
        });
        let subscription =
            cx.subscribe_in(&editor, window, |viewer, _editor, event: &CommentEditorEvent, window, cx| match event {
                CommentEditorEvent::Changed(_) => cx.notify(),
                CommentEditorEvent::Submit => viewer.finish_repository_commit(window, cx),
                CommentEditorEvent::Cancel => {
                    viewer.handle_command(ReviewCommand::Cancel, window, cx);
                }
            });
        editor.focus_handle(cx).focus(window, cx);
        self.overlay = Some(Overlay::Repository(RepositoryPrompt::Commit { editor, _subscription: subscription }));
        cx.notify();
    }

    fn open_discard_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(file) = self.selected_file().and_then(|index| self.document().files.get(index)) else {
            return;
        };
        let prompt = RepositoryPrompt::Discard { path: file.path.clone(), status: file.status };
        self.overlay = Some(Overlay::Repository(prompt));
        cx.notify();
    }

    fn finish_repository_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Overlay::Repository(RepositoryPrompt::Commit { editor, .. })) = &self.overlay else {
            return;
        };
        let message = editor.read(cx).body().trim().to_owned();
        if message.is_empty() {
            self.repository_error = Some("Commit message cannot be empty".to_owned());
            cx.notify();
            return;
        }
        self.overlay = None;
        self.handle_command(DiffReviewCommand::RepositoryAction(RepositoryAction::Commit { message }), window, cx);
        cx.notify();
    }

    fn confirm_discard(&mut self, _: &ConfirmDiscard, window: &mut Window, cx: &mut Context<Self>) {
        let discarding =
            |overlay: &mut Overlay| matches!(overlay, Overlay::Repository(RepositoryPrompt::Discard { .. }));
        let Some(Overlay::Repository(RepositoryPrompt::Discard { path, status })) = self.overlay.take_if(discarding)
        else {
            return;
        };
        self.handle_command(
            DiffReviewCommand::RepositoryAction(RepositoryAction::Discard { path, status }),
            window,
            cx,
        );
        cx.notify();
    }

    fn cancel_repository_prompt(&mut self, _: &CancelRepositoryPrompt, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(ReviewCommand::Cancel, window, cx);
    }

    fn run_command(&mut self, action: &RunCommand, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(action.0.clone(), window, cx);
    }

    fn submit_comment(&mut self, _: &SubmitComment, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(ReviewCommand::SubmitComment, window, cx);
    }

    fn cancel_comment(&mut self, _: &CancelComment, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(ReviewCommand::Cancel, window, cx);
    }

    fn render_repository_prompt(&self) -> Option<impl IntoElement> {
        let Some(Overlay::Repository(prompt)) = &self.overlay else {
            return None;
        };
        let palette = &self.theme.diff;
        let hint = |text| div().mt_2().text_color(color(palette.muted)).child(text);
        Some(match prompt {
            RepositoryPrompt::Commit { editor, .. } => {
                Modal::new("repository-prompt", "Commit staged changes", self.ui_theme())
                    .child(editor.clone())
                    .child(hint("Enter to commit · Esc to cancel"))
            }
            RepositoryPrompt::Discard { path, .. } => {
                Modal::new("repository-prompt", format!("Discard changes to {path}?"), self.ui_theme())
                    .child(
                        div()
                            .text_color(color(palette.deletion))
                            .child("This removes both staged and unstaged changes."),
                    )
                    .child(hint("y to confirm · n or Esc to cancel"))
            }
        })
    }
}

fn keystroke(chord: Chord) -> String {
    let key = match chord.key {
        Key::Char(' ') => "space".to_owned(),
        Key::Char('?') => "shift-/".to_owned(),
        Key::Char(character) if character.is_ascii_uppercase() => format!("shift-{}", character.to_ascii_lowercase()),
        Key::Char(character) => character.to_string(),
        Key::Esc => "escape".to_owned(),
        key => format!("{key:?}").to_lowercase(),
    };
    if chord.ctrl { format!("ctrl-{key}") } else { key }
}

fn key_context(chord: Chord, scope: KeyScope) -> String {
    let scope = match scope {
        KeyScope::Any => "",
        KeyScope::Navigation => " && pane == files",
        KeyScope::Document => " && pane == diff",
        KeyScope::Split => " && pane == diff && layout == split",
        KeyScope::Unified => " && pane == diff && layout == unified",
        KeyScope::Card => " && card",
        KeyScope::Reference => " && reference && !card",
    };
    let activates = matches!(chord.key, Key::Enter | Key::Tab | Key::Char(' '));
    let controls = if activates { " && !CommentDismissControl && !StageCheckbox" } else { "" };
    format!("DiffViewer && mode == browse{scope}{controls}")
}

impl EventEmitter<DiffViewerEvent> for DiffViewer {}
impl EventEmitter<ThemeChanged> for DiffViewer {}
impl EventEmitter<ViewModeChanged> for DiffViewer {}

fn clamp_rems(width: Rems, min: Rems, max: Rems) -> Rems {
    rems(width.0.clamp(min.0, max.0))
}

fn clamp_sidebar_width(width: Rems, available: Rems) -> Rems {
    let maximum = rems(MAX_SIDEBAR_WIDTH.0.min((available - MIN_DIFF_WIDTH).0.max(0.0)));
    if maximum.0 < MIN_SIDEBAR_WIDTH.0 {
        clamp_rems(width, Rems::ZERO, maximum)
    } else {
        clamp_rems(width, MIN_SIDEBAR_WIDTH, maximum)
    }
}

impl Render for DiffViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rem_size = window.rem_size();
        if self.rem_size != rem_size {
            self.rem_size = rem_size;
            self.diff_list_state.remeasure();
        }
        let viewport_width = rems(window.viewport_size().width / rem_size);
        let preferred_width = self
            .sidebar
            .preferred_width
            .unwrap_or_else(|| rems((viewport_width.0 * DEFAULT_SIDEBAR_FRACTION).max(DEFAULT_MIN_SIDEBAR_WIDTH.0)));
        let sidebar_width = clamp_sidebar_width(preferred_width, viewport_width);
        if self.sidebar.width != sidebar_width {
            self.sidebar.width = sidebar_width;
            self.diff_list_state.remeasure();
        }
        let diff_width = if self.sidebar.collapsed { viewport_width } else { viewport_width - sidebar_width };
        let split_when_auto = diff_width.0 >= self.options.auto_split_width.0;
        if self.session.split_when_auto() != split_when_auto {
            self.change_layout(|session| session.set_split_when_auto(split_when_auto));
        }
        self.follow_active_file();
        let palette = self.theme.diff.clone();
        let focus_handle = self.focus_handle.get_or_insert_with(|| cx.focus_handle()).clone();
        let editing_commit = matches!(self.overlay, Some(Overlay::Repository(RepositoryPrompt::Commit { .. })));
        if window.focused(cx).is_none() && self.comment_editor.is_none() && !editing_commit {
            focus_handle.focus(window, cx);
        }
        let mode = match &self.overlay {
            Some(overlay) => overlay.mode(),
            None if self.comment_editor.is_some() => "draft",
            None => "browse",
        };
        let pane = match self.pane {
            ViewerPane::Files => "files",
            ViewerPane::Diff => "diff",
        };
        let layout = if self.session.selected_layout().is_split() { "split" } else { "unified" };
        let mut key_context = KeyContext::default();
        key_context.add("DiffViewer");
        key_context.set("mode", mode);
        key_context.set("pane", pane);
        key_context.set("layout", layout);
        if self.names.card().is_some() {
            key_context.add("card");
        }
        if self.session.in_reference() {
            key_context.add("reference");
        }
        div()
            .debug_selector(|| "diff-viewer".to_owned())
            .key_context(key_context)
            .track_focus(&focus_handle)
            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                focus_handle.focus(window, cx);
            })
            .on_action(cx.listener(Self::run_command))
            .on_action(cx.listener(Self::submit_comment))
            .on_action(cx.listener(Self::cancel_comment))
            .on_action(cx.listener(Self::confirm_discard))
            .on_action(cx.listener(Self::cancel_repository_prompt))
            .on_modifiers_changed(cx.listener(Self::modifiers_changed))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(color(palette.background))
            .text_color(color(palette.foreground))
            .font_family(UI_FONT)
            .typography(Typography::Body)
            .child(self.render_toolbar(window, cx))
            .map(|viewer| {
                if self.document().files.is_empty() {
                    return viewer.child(self.render_clean());
                }
                viewer
                    .child(
                        div()
                            .id("diff-viewer-content")
                            .debug_selector(|| "diff-viewer-content".to_owned())
                            .flex_1()
                            .overflow_hidden()
                            .flex()
                            .children((!self.sidebar.collapsed).then(|| self.render_sidebar(cx)))
                            .child(match self.session.reference_status() {
                                Some(status) => self.render_reference_status(status).into_any_element(),
                                None => self.render_diff(cx).into_any_element(),
                            }),
                    )
                    .when(self.options.review_bar, |viewer| viewer.child(self.render_review_bar(cx)))
            })
            .children(self.render_picker_overlay(cx))
            .when_some(self.theme_picker_overlay(), |viewer, picker| viewer.child(picker.render(self.ui_theme())))
            .children(self.render_repository_prompt())
            .when_some(self.repository_error().map(str::to_owned), |viewer, error| {
                viewer.child(
                    div().absolute().bottom_4().left_4().right_4().child(Notification::error(error, self.ui_theme())),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clankerdiff_core::testing::DocumentBuilder;

    fn assert_close(actual: Rems, expected_px: f32) {
        assert!((actual.0 - rem(expected_px).0).abs() < f32::EPSILON);
    }

    #[test]
    fn sidebar_width_is_clamped_to_preserve_the_diff_pane() {
        assert_close(clamp_sidebar_width(rem(100.0), rem(1_000.0)), 180.0);
        assert_close(clamp_sidebar_width(rem(400.0), rem(1_000.0)), 400.0);
        assert_close(clamp_sidebar_width(rem(900.0), rem(1_000.0)), 680.0);
        assert_close(clamp_sidebar_width(rem(400.0), rem(600.0)), 280.0);
    }

    #[test]
    fn sidebar_can_shrink_below_its_normal_minimum_in_a_narrow_viewport() {
        assert_close(clamp_sidebar_width(rem(280.0), rem(450.0)), 130.0);
        assert_close(clamp_sidebar_width(rem(280.0), rem(300.0)), 0.0);
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
        let fixture = DocumentBuilder::new().changed("src/large.rs", &source_lines(""), &source_lines(" + 1")).build();
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
        assert_eq!(viewer.highlight_stats().misses, 2, "each complete side is parsed once across repeated frames");
    }
}
