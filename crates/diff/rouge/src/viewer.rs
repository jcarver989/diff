mod commands;
#[path = "navigation.rs"]
mod navigation;
mod overlay;

use crate::syntax_job::SyntaxJob;
use crate::{DiffViewerEvent, shortcuts::Shortcuts};
use clankerdiff_core::{
    CommandContext, DiffDocument, DiffReviewCommand, DiffSide, FileTree, FilesMode, Layout, LineAnchor, LookupReply,
    RepoPath, ReviewCapabilities, ReviewSession, ViewMode, ViewportAnchor,
};
use clankerdiff_rouge_ui::theme::ActiveTheme;
use clankerdiff_symbols::{DefinitionCard, DocumentSyntax, NameLookup};
use clankerdiff_ui_core::ReviewTheme;
use overlay::{Overlay, OverlayKind, PickerCommand};
use rouge_core::{
    Align, BuildCx, ElementId, Key, KeyEvent, Rect, Styled, View, VirtualListHandle, div, dynamic, focus_handle,
    root_font_size,
};
use rouge_reactive::{ReadSignal, Signal, Task, batch, effect, signal};
use std::{mem, rc::Rc, sync::Arc};

pub use clankerdiff_core::FocusPane as ViewerPane;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCommand(pub DiffReviewCommand);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffViewerOptions {
    pub sidebar_width: Option<f32>,
    pub auto_split_width: f32,
    pub themes: bool,
    pub review_bar: bool,
    pub view_mode: ViewMode,
}

impl Default for DiffViewerOptions {
    fn default() -> Self {
        Self {
            sidebar_width: None,
            auto_split_width: 900.0 / 16.0,
            themes: true,
            review_bar: true,
            view_mode: ViewMode::Auto,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewOutput {
    Event(DiffViewerEvent),
    ThemeChanged(String),
    ViewModeChanged(ViewMode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub pane: ViewerPane,
    pub file: Option<usize>,
    pub row: Option<usize>,
    pub side: DiffSide,
    pub sidebar_row: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewStatus {
    pub context: CommandContext,
    pub repository_error: Option<String>,
    pub sidebar_collapsed: bool,
    pub files_mode: FilesMode,
    pub view_mode: ViewMode,
    pub layout: Layout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollRequest {
    pub pane: ViewerPane,
    pub lines: isize,
}

pub struct ReviewController {
    session: ReviewSession,
    sidebar: FileTree,
    browse: Option<FileTree>,
    pane: ViewerPane,
    sidebar_collapsed: bool,
    page_rows: [isize; 2],
    root_width_rem: Option<f32>,
    sidebar_width_rem: f32,
    sidebar_preferred_rem: Option<f32>,
    sidebar_revision: u64,
    capabilities: ReviewCapabilities,
    repository_pending: bool,
    repository_error: Option<String>,
    background_error: Option<String>,
    theme: ReviewTheme,
    options: DiffViewerOptions,
    overlay: Option<Overlay>,
    names: NameLookup,
    syntax: Arc<DocumentSyntax>,
    syntax_job: Option<SyntaxJob>,
    focus_file: Option<usize>,
    definition_revision: u64,
    hide_card: Option<Task>,
    outputs: Vec<ReviewOutput>,
    scroll: Option<ScrollRequest>,
    picker_command: Option<PickerCommand>,
}

impl ReviewController {
    #[must_use]
    pub fn new(document: Arc<DiffDocument>) -> Self {
        Self::with_options(document, ReviewTheme::default(), DiffViewerOptions::default())
    }

    #[must_use]
    pub fn with_options(document: Arc<DiffDocument>, theme: ReviewTheme, options: DiffViewerOptions) -> Self {
        let mut sidebar = FileTree::new(&document);
        let mut session = ReviewSession::new(document);
        session.set_view_mode(options.view_mode);
        if let Some(file) = session.selected_file() {
            sidebar.reveal_file(file);
        }
        Self {
            session,
            sidebar,
            browse: None,
            pane: ViewerPane::Diff,
            sidebar_collapsed: false,
            page_rows: [1, 1],
            root_width_rem: None,
            sidebar_width_rem: 0.0,
            sidebar_preferred_rem: None,
            sidebar_revision: 0,
            capabilities: ReviewCapabilities::default(),
            repository_pending: false,
            repository_error: None,
            background_error: None,
            theme,
            options,
            overlay: None,
            names: NameLookup::default(),
            syntax: Arc::default(),
            syntax_job: None,
            focus_file: None,
            definition_revision: 0,
            hide_card: None,
            outputs: Vec::new(),
            scroll: None,
            picker_command: None,
        }
    }

    #[must_use]
    pub fn mount(self, output: impl Fn(ReviewOutput) + 'static) -> ReviewHandle {
        ReviewHandle {
            selection: signal(self.selection()),
            status: signal(self.status()),
            sidebar_width: signal(self.sidebar_width_rem),
            sidebar_revision: signal(self.sidebar_revision),
            projection: signal(self.session.projection_revision()),
            annotations: signal(self.session.annotations_revision()),
            comments: signal(0),
            draft: signal(None),
            viewport: signal(None),
            reveal: signal(None),
            theme: signal(self.theme.clone()),
            overlay: signal(self.overlay_kind()),
            definition: signal(self.definition_revision),
            syntax: signal(0),
            scroll: signal(None),
            picker: signal(None),
            commit_anchor: signal(Rect::ZERO),
            state: signal(self),
            output: Rc::new(output),
        }
    }

    #[must_use]
    pub const fn session(&self) -> &ReviewSession {
        &self.session
    }

    #[must_use]
    pub fn symbols_ready(&self) -> bool {
        Arc::ptr_eq(self.syntax.document(), self.session.presentation().document())
            && self.syntax.ready_symbols().is_some()
    }

    #[must_use]
    pub const fn definition_card(&self) -> Option<&DefinitionCard> {
        self.names.card()
    }

    #[must_use]
    pub const fn options(&self) -> DiffViewerOptions {
        self.options
    }

    #[must_use]
    pub fn selection(&self) -> Selection {
        Selection {
            pane: self.pane,
            file: self.session.selected_file(),
            row: self.session.selected_row(),
            side: self.session.selected_side(),
            sidebar_row: self.tree().and_then(FileTree::selected_position),
        }
    }

    #[must_use]
    pub fn status(&self) -> ReviewStatus {
        ReviewStatus {
            context: self.command_context(),
            repository_error: self.repository_error.clone().or_else(|| self.background_error.clone()),
            sidebar_collapsed: self.sidebar_collapsed,
            files_mode: self.session.files_mode(),
            view_mode: self.session.view_mode(),
            layout: self.session.selected_layout(),
        }
    }

    fn update_width(&mut self) {
        let Some(width) = self.root_width_rem else {
            return;
        };
        let rem = |px| px / clankerdiff_rouge_ui::tokens::BASE_SIZE;
        let maximum = (width - rem(320.0)).clamp(0.0, rem(960.0));
        let preferred =
            self.sidebar_preferred_rem.or(self.options.sidebar_width).unwrap_or_else(|| (width * 0.25).max(rem(320.0)));
        self.sidebar_width_rem = preferred.clamp(rem(180.0).min(maximum), maximum);
        let diff_width = if self.sidebar_collapsed { width } else { width - self.sidebar_width_rem };
        self.session.set_split_when_auto(diff_width >= self.options.auto_split_width);
    }

    fn emit(&mut self, event: DiffViewerEvent) {
        if matches!(
            event,
            DiffViewerEvent::RepositoryAction(_) | DiffViewerEvent::SetTarget(_) | DiffViewerEvent::Refresh
        ) {
            self.repository_pending = true;
            self.repository_error = None;
        }
        self.outputs.push(ReviewOutput::Event(event));
    }
}

#[derive(Clone)]
pub struct ReviewHandle {
    state: Signal<ReviewController>,
    selection: Signal<Selection>,
    status: Signal<ReviewStatus>,
    sidebar_width: Signal<f32>,
    sidebar_revision: Signal<u64>,
    projection: Signal<u64>,
    annotations: Signal<u64>,
    comments: Signal<u64>,
    draft: Signal<Option<(LineAnchor, Option<u64>)>>,
    viewport: Signal<Option<VirtualListHandle>>,
    reveal: Signal<Option<(usize, Align)>>,
    theme: Signal<ReviewTheme>,
    overlay: Signal<OverlayKind>,
    definition: Signal<u64>,
    syntax: Signal<u64>,
    scroll: Signal<Option<ScrollRequest>>,
    picker: Signal<Option<clankerdiff_rouge_ui::picker::SearchPickerHandle>>,
    commit_anchor: Signal<Rect>,
    output: Rc<dyn Fn(ReviewOutput)>,
}

impl ReviewHandle {
    pub fn with<T>(&self, f: impl FnOnce(&ReviewController) -> T) -> T {
        self.state.with_untracked(f)
    }

    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.state.is_alive()
    }

    #[must_use]
    pub fn selection(&self) -> ReadSignal<Selection> {
        self.selection.read_only()
    }
    #[must_use]
    pub fn sidebar_revision(&self) -> ReadSignal<u64> {
        self.sidebar_revision.read_only()
    }

    #[must_use]
    pub fn sidebar_width_rem(&self) -> ReadSignal<f32> {
        self.sidebar_width.read_only()
    }

    #[must_use]
    pub fn status(&self) -> ReadSignal<ReviewStatus> {
        self.status.read_only()
    }
    #[must_use]
    pub fn presentation_revision(&self) -> ReadSignal<u64> {
        self.projection.read_only()
    }
    #[must_use]
    pub fn annotations_revision(&self) -> ReadSignal<u64> {
        self.annotations.read_only()
    }
    #[must_use]
    pub fn theme(&self) -> ReadSignal<ReviewTheme> {
        self.theme.read_only()
    }
    #[must_use]
    pub fn definition_revision(&self) -> ReadSignal<u64> {
        self.definition.read_only()
    }
    #[must_use]
    pub fn scroll_request(&self) -> ReadSignal<Option<ScrollRequest>> {
        self.scroll.read_only()
    }

    pub(crate) const fn commit_anchor(&self) -> Signal<Rect> {
        self.commit_anchor
    }

    pub(crate) fn comments_revision(&self) -> ReadSignal<u64> {
        self.comments.read_only()
    }

    pub(crate) fn draft_placement(&self) -> ReadSignal<Option<(LineAnchor, Option<u64>)>> {
        self.draft.read_only()
    }

    pub(crate) fn syntax_revision(&self) -> ReadSignal<u64> {
        self.syntax.read_only()
    }

    pub(crate) fn with_syntax<T>(&self, f: impl FnOnce(&ReviewController, &DocumentSyntax) -> T) -> T {
        self.with(|state| f(state, &state.syntax))
    }

    pub(crate) fn resize_sidebar(&self, width: Option<f32>) {
        self.update(|state| {
            state.sidebar_preferred_rem = width;
            state.update_width();
        });
    }

    pub(crate) fn bind_viewport(&self, list: VirtualListHandle) {
        self.viewport.set(Some(list));
    }

    pub(crate) fn reveal_row(&self) -> ReadSignal<Option<(usize, Align)>> {
        self.reveal.read_only()
    }

    pub(crate) fn reveal_selection(&self, align: Align) {
        self.reveal.set(self.selection.get_untracked().row.map(|row| (row, align)));
    }

    pub(crate) fn set_page_rows(&self, diff: usize, files: usize) {
        self.state.try_update(|state| {
            let rows = |count: usize| isize::try_from(count.max(1)).unwrap_or(isize::MAX);
            state.page_rows = [rows(diff), rows(files)];
        });
    }

    fn update<T>(&self, f: impl FnOnce(&mut ReviewController) -> T) -> Option<T> {
        if !self.is_alive() {
            return None;
        }
        batch(|| {
            let (result, outputs, picker_command) = self.state.try_update(|state| {
                let previous_file = state.session.selected_file();
                let before = state.session.annotations_revision();
                let projection = state.session.projection_revision();
                let viewport = self.viewport.get_untracked().and_then(|list| {
                    rouge_reactive::untrack(|| {
                        let scroll = list.scroll_state().try_get()?;
                        if scroll.viewport.height == 0.0 {
                            return None;
                        }
                        let top = list.top_row().get();
                        let row = state.session.visible_range().start + top;
                        let anchor = ViewportAnchor::capture(state.session.presentation(), row)?;
                        Some((list, anchor, scroll.offset.y - list.row_top(top)))
                    })
                });
                let result = f(state);
                state.follow_syntax();
                if !state.syntax_job.as_ref().is_some_and(|job| job.targets(&state.syntax)) {
                    let handle = self.state;
                    state.syntax_job = Some(SyntaxJob::start(&state.syntax, self.syntax, move || {
                        handle.is_alive().then(|| handle.with_untracked(|state| state.focus_file)).flatten()
                    }));
                }
                if before != state.session.annotations_revision() {
                    self.comments.update(|revision| *revision = revision.wrapping_add(1));
                }
                self.draft.set_if_changed(state.session.draft().map(|draft| (draft.anchor().clone(), draft.editing())));
                if state.pane == ViewerPane::Files && state.sidebar_collapsed {
                    state.sidebar_collapsed = false;
                    state.update_width();
                }
                if state.pane == ViewerPane::Diff
                    && state.session.files_mode() == FilesMode::Diff
                    && let Some(file) = state.session.selected_file().filter(|file| Some(*file) != previous_file)
                {
                    state.sidebar.follow_file(file);
                }
                self.sidebar_revision.set_if_changed(state.sidebar_revision);
                self.selection.set_if_changed(state.selection());
                self.sidebar_width.set_if_changed(state.sidebar_width_rem);
                self.status.set_if_changed(state.status());
                self.projection.set_if_changed(state.session.projection_revision());
                if projection != state.session.projection_revision()
                    && let Some((list, anchor, offset)) = viewport
                {
                    let range = state.session.visible_range();
                    if let Some(row) = anchor.resolve(state.session.presentation(), range.clone()) {
                        list.scroll_to_row_offset(row - range.start, offset);
                    }
                }
                self.annotations.set_if_changed(state.session.annotations_revision());
                self.definition.set_if_changed(state.definition_revision);
                self.overlay.set_if_changed(state.overlay_kind());
                if self.theme.with_untracked(ReviewTheme::revision) != state.theme.revision() {
                    self.theme.set(state.theme.clone());
                }
                if let Some(scroll) = state.scroll.take() {
                    self.scroll.set(Some(scroll));
                }
                (result, mem::take(&mut state.outputs), state.picker_command.take())
            })?;
            if let Some(command) = picker_command
                && let Some(picker) = self.picker.get_untracked()
            {
                command.apply(&picker);
            }
            for output in outputs {
                (self.output)(output);
            }
            Some(result)
        })
    }

    pub fn handle_command(&self, command: impl Into<DiffReviewCommand>) -> bool {
        use clankerdiff_core::{DiffReviewCommand as C, ReviewCommand as R};
        if !self.is_alive() {
            return false;
        }
        let command = command.into();
        let align = match command {
            C::SelectFile(_) => Some(Align::Top),
            C::MoveSelection(_)
            | C::Page(_)
            | C::First
            | C::Last
            | C::SelectRow(_)
            | C::JumpTo(_)
            | C::GoToDefinition
            | C::NavigateBack
            | C::NavigateForward
            | C::ToggleSourceView
            | C::SetFilesMode(_)
            | C::Review(R::BeginComment | R::EditComment) => Some(Align::Visible),
            _ => None,
        };
        batch(|| {
            let old = self.selection.get_untracked();
            let handled = self.update(|state| state.handle_command(command)).unwrap_or(false);
            let next = self.selection.get_untracked();
            if handled
                && let Some(align) = align
                && (next.row != old.row || next.file != old.file || next.pane == ViewerPane::Diff)
            {
                self.reveal_selection(align);
            }
            handled
        })
    }

    pub fn set_document(&self, document: Arc<DiffDocument>) {
        self.update(|state| state.set_document(document));
    }
    pub fn set_repository_files(&self, paths: &[RepoPath]) {
        self.update(|state| state.set_repository_files(paths));
    }
    pub fn set_capabilities(&self, capabilities: ReviewCapabilities) {
        self.update(|state| state.set_capabilities(capabilities));
    }
    pub fn set_theme(&self, theme: ReviewTheme) {
        self.update(|state| {
            state.theme = theme;
        });
    }
    pub fn set_background_error(&self, error: Option<String>) {
        self.update(|state| {
            state.background_error = error;
        });
    }
    pub fn clear_review(&self) {
        self.update(|state| state.session.clear_review());
    }
    pub fn command_finished(&self, result: Result<(), String>) {
        self.update(|state| {
            state.repository_pending = false;
            state.repository_error = result.err();
        });
    }
    pub fn lookup_finished(&self, reply: LookupReply) {
        self.update(|state| state.lookup_finished(reply));
    }
    pub fn set_draft_body(&self, body: impl Into<String>) {
        let body = body.into();
        batch(|| {
            self.state.try_update(|state| {
                if state.session.draft().is_some_and(|draft| draft.body() != body)
                    && let Some(draft) = state.session.draft_mut()
                {
                    draft.set_body(body);
                    self.annotations.set_if_changed(state.session.annotations_revision());
                }
            });
        });
    }
    pub fn set_viewport(&self, diff_width_rem: f32, diff_rows: usize, files_rows: usize) {
        self.update(|state| {
            let rows = |count: usize| isize::try_from(count.max(1)).unwrap_or(isize::MAX);
            state.page_rows = [rows(diff_rows), rows(files_rows)];
            state.session.set_split_when_auto(diff_width_rem >= state.options.auto_split_width);
        });
    }
}

#[must_use]
pub fn diff_view(handle: ReviewHandle) -> impl View {
    let content = handle.clone();
    diff_view_with(handle, move || crate::diff_view::content(content, ActiveTheme::current().ui()))
}

#[must_use]
pub fn diff_view_with<T: View>(handle: ReviewHandle, content: impl FnOnce() -> T + 'static) -> impl View {
    ReviewView { handle, content }
}

struct ReviewView<T> {
    handle: ReviewHandle,
    content: T,
}

impl<T: View, U: FnOnce() -> T + 'static> View for ReviewView<U> {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let Self { handle, content } = self;
        let bounds = signal(Rect::default());
        let font_size = root_font_size();
        effect({
            let handle = handle.clone();
            move || {
                let width = bounds.get().size.width / font_size.get();
                if width.is_finite() {
                    handle.update(|state| {
                        state.root_width_rem = Some(width);
                        state.update_width();
                    });
                }
            }
        });
        let active = ActiveTheme::install(handle.theme.get_untracked());
        effect({
            let theme = handle.theme();
            move || active.set(theme.get())
        });
        let shortcuts = Shortcuts::new();
        let focus = focus_handle();
        let was_draft = std::cell::Cell::new(false);
        effect({
            let overlay = handle.overlay;
            move || {
                let drafting = overlay.get() == OverlayKind::Draft;
                if was_draft.replace(drafting) && !drafting {
                    focus.focus();
                }
            }
        });
        handle.update(|_| ());
        div()
            .test_id("diff-viewer")
            .size_full()
            .relative()
            .focusable()
            .track_focus(focus)
            .track_bounds(bounds)
            .autofocus()
            .on_action({
                let handle = handle.clone();
                move |action: &RunCommand| {
                    handle.handle_command(action.0.clone());
                }
            })
            .on_key_down({
                let handle = handle.clone();
                move |event| handle.key_down(event, &shortcuts)
            })
            .child(content())
            .child(dynamic(move || {
                let kind = handle.overlay.get();
                overlay::view(&handle, kind, active.ui())
            }))
            .build(cx)
    }
}

impl ReviewHandle {
    fn key_down(&self, event: &KeyEvent, shortcuts: &Shortcuts) {
        use clankerdiff_core::{InteractionPhase, KeyState, ReviewCommand};
        let (phase, keys) = self.with(|state| {
            (state.command_context().phase, KeyState::new(state.pane, &state.session, state.names.card().is_some()))
        });
        let handled = if phase == InteractionPhase::Browse {
            shortcuts.resolve(event, keys).is_some_and(|command| self.handle_command(command.clone()))
        } else if event.key == Key::Escape {
            self.handle_command(ReviewCommand::Cancel)
        } else {
            false
        };
        if handled || (phase != InteractionPhase::Browse && event.key == Key::Tab) {
            event.stop_propagation();
        }
    }
}
