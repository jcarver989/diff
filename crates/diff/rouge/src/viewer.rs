mod commands;
#[path = "navigation.rs"]
mod navigation;
mod overlay;

use crate::theme::ActiveReviewTheme;
use crate::{DiffViewerEvent, diff_view::content, shortcuts::Shortcuts, syntax_job};
use clankerdiff_core::{
    CommandContext, DiffDocument, DiffReviewCommand, DiffSide, FileTree, FilesMode, Layout, LineAnchor, LookupReply,
    RepoPath, ReviewCapabilities, ReviewSession, ViewMode, ViewportAnchor,
};
use clankerdiff_symbols::{DefinitionCard, DocumentSyntax, NameLookup};
use clankerdiff_ui_core::ReviewTheme;
use overlay::{Overlay, OverlayKind, PickerCommand};
use rouge_core::{
    Align, BuildCx, ElementId, Key, KeyEvent, Rect, Styled, View, VirtualListHandle, div, dynamic, focus_handle,
    root_font_size, view,
};
use rouge_reactive::{
    Callback, Memo, ReadSignal, Signal, Stored, Task, batch, effect, memo, signal, spawn, stored, untrack,
};
use rouge_widgets::{SearchPickerHandle, tokens::BASE_SIZE};
use std::{cell::Cell, mem, ops::Range, sync::Arc};

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
    pub wrap: bool,
}

impl Default for DiffViewerOptions {
    fn default() -> Self {
        Self {
            sidebar_width: None,
            auto_split_width: 900.0 / 16.0,
            themes: true,
            review_bar: true,
            view_mode: ViewMode::Auto,
            wrap: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewOutput {
    Event(DiffViewerEvent),
    ThemeChanged(String),
    ViewModeChanged(ViewMode),
    WrapChanged(bool),
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
    pub wrap: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollRequest {
    pub pane: ViewerPane,
    pub lines: isize,
}

enum Effect {
    Output(ReviewOutput),
    Scroll(ScrollRequest),
    Picker(PickerCommand),
}

pub struct ReviewController {
    session: ReviewSession,
    sidebar: FileTree,
    browse: Option<FileTree>,
    pane: ViewerPane,
    sidebar_collapsed: bool,
    wrap: bool,
    capabilities: ReviewCapabilities,
    repository_pending: bool,
    repository_error: Option<String>,
    background_error: Option<String>,
    theme: ReviewTheme,
    options: DiffViewerOptions,
    overlay: Option<Overlay>,
    names: NameLookup,
    syntax: Arc<DocumentSyntax>,
    sidebar_revision: u64,
    definition_revision: u64,
    comments_revision: u64,
    hide_card: Option<Task>,
    effects: Vec<Effect>,
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
        let mut controller = Self {
            session,
            sidebar,
            browse: None,
            pane: ViewerPane::Diff,
            sidebar_collapsed: false,
            wrap: options.wrap,
            capabilities: ReviewCapabilities::default(),
            repository_pending: false,
            repository_error: None,
            background_error: None,
            theme,
            options,
            overlay: None,
            names: NameLookup::default(),
            syntax: Arc::default(),
            sidebar_revision: 0,
            definition_revision: 0,
            comments_revision: 0,
            hide_card: None,
            effects: Vec::new(),
        };
        controller.follow_syntax();
        controller
    }

    #[must_use]
    pub fn mount(mut self, output: impl Fn(ReviewOutput) + 'static) -> ReviewHandle {
        let theme = signal(mem::take(&mut self.theme));
        let options = self.options;
        let state = signal(self);
        let read = state.read_only();
        let width = signal(None);
        let preferred = signal(None);
        let handle = ReviewHandle {
            state,
            theme,
            width,
            preferred,
            focus_file: signal(None),
            syntax: signal(0),
            reveal: signal(None),
            viewport: stored(None),
            picker: stored(None),
            commit_anchor: signal(Rect::ZERO),
            selection: read.select(Self::selection),
            status: read.select(Self::status),
            projection: read.select(|state| state.session.projection_revision()),
            annotations: read.select(|state| state.session.annotations_revision()),
            comments: read.select(|state| state.comments_revision),
            draft: read.select(|state| state.session.draft().map(|draft| (draft.anchor().clone(), draft.editing()))),
            overlay: read.select(Self::overlay_kind),
            definition: read.select(|state| state.definition_revision),
            sidebar_revision: read.select(|state| state.sidebar_revision),
            sidebar_width: memo(move || sidebar_width(width.get(), preferred.get(), options)),
            output: output.into(),
        };
        handle.split_when_wide();
        handle.prepare_syntax();
        handle
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
            wrap: self.wrap,
        }
    }

    fn reconcile(&mut self, previous_file: Option<usize>, annotations: u64) {
        self.follow_syntax();
        if annotations != self.session.annotations_revision() {
            self.comments_revision = self.comments_revision.wrapping_add(1);
        }
        if self.pane == ViewerPane::Files {
            self.sidebar_collapsed = false;
        }
        if self.pane == ViewerPane::Diff
            && self.session.files_mode() == FilesMode::Diff
            && let Some(file) = self.session.selected_file().filter(|file| Some(*file) != previous_file)
        {
            self.sidebar.follow_file(file);
        }
    }

    fn emit(&mut self, event: DiffViewerEvent) {
        if matches!(
            event,
            DiffViewerEvent::RepositoryAction(_) | DiffViewerEvent::SetTarget(_) | DiffViewerEvent::Refresh
        ) {
            self.repository_pending = true;
            self.repository_error = None;
        }
        self.effects.push(Effect::Output(ReviewOutput::Event(event)));
    }
}

fn sidebar_width(width: Option<f32>, preferred: Option<f32>, options: DiffViewerOptions) -> f32 {
    let Some(width) = width else {
        return 0.0;
    };
    let rem = |px| px / BASE_SIZE;
    let maximum = (width - rem(320.0)).clamp(0.0, rem(960.0));
    let preferred = preferred.or(options.sidebar_width).unwrap_or_else(|| (width * 0.25).max(rem(320.0)));
    preferred.clamp(rem(180.0).min(maximum), maximum)
}

#[derive(Clone, Copy)]
pub(crate) struct Viewport {
    pub(crate) diff: VirtualListHandle,
    pub(crate) files: VirtualListHandle,
    pub(crate) visible: Signal<Range<usize>>,
    pub(crate) root_font_size: Signal<f32>,
}

#[derive(Clone, Copy)]
pub struct ReviewHandle {
    state: Signal<ReviewController>,
    theme: Signal<ReviewTheme>,
    width: Signal<Option<f32>>,
    preferred: Signal<Option<f32>>,
    focus_file: Signal<Option<usize>>,
    syntax: Signal<u64>,
    reveal: Signal<Option<(usize, Align)>>,
    viewport: Stored<Option<Viewport>>,
    picker: Stored<Option<SearchPickerHandle>>,
    commit_anchor: Signal<Rect>,
    selection: Memo<Selection>,
    status: Memo<ReviewStatus>,
    projection: Memo<u64>,
    annotations: Memo<u64>,
    comments: Memo<u64>,
    draft: Memo<Option<(LineAnchor, Option<u64>)>>,
    overlay: Memo<OverlayKind>,
    definition: Memo<u64>,
    sidebar_revision: Memo<u64>,
    sidebar_width: Memo<f32>,
    output: Callback<(ReviewOutput,)>,
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
    pub const fn selection(&self) -> Memo<Selection> {
        self.selection
    }

    #[must_use]
    pub const fn sidebar_revision(&self) -> Memo<u64> {
        self.sidebar_revision
    }

    #[must_use]
    pub const fn sidebar_width_rem(&self) -> Memo<f32> {
        self.sidebar_width
    }

    #[must_use]
    pub const fn status(&self) -> Memo<ReviewStatus> {
        self.status
    }

    #[must_use]
    pub const fn presentation_revision(&self) -> Memo<u64> {
        self.projection
    }

    #[must_use]
    pub const fn annotations_revision(&self) -> Memo<u64> {
        self.annotations
    }

    #[must_use]
    pub fn theme(&self) -> ReadSignal<ReviewTheme> {
        self.theme.read_only()
    }

    #[must_use]
    pub const fn definition_revision(&self) -> Memo<u64> {
        self.definition
    }

    pub(crate) const fn commit_anchor(&self) -> Signal<Rect> {
        self.commit_anchor
    }

    #[must_use]
    pub const fn comments_revision(&self) -> Memo<u64> {
        self.comments
    }

    pub(crate) const fn draft_placement(&self) -> Memo<Option<(LineAnchor, Option<u64>)>> {
        self.draft
    }

    pub(crate) fn syntax_revision(&self) -> ReadSignal<u64> {
        self.syntax.read_only()
    }

    pub(crate) fn with_syntax<T>(&self, f: impl FnOnce(&ReviewController, &DocumentSyntax) -> T) -> T {
        self.with(|state| f(state, &state.syntax))
    }

    pub(crate) fn resize_sidebar(&self, width: Option<f32>) {
        self.preferred.set_if_changed(width);
    }

    pub(crate) fn bind_viewport(&self, viewport: Viewport) {
        self.viewport.set(Some(viewport));
    }

    pub(crate) fn reveal_row(&self) -> ReadSignal<Option<(usize, Align)>> {
        self.reveal.read_only()
    }

    pub(crate) fn reveal_selection(&self, align: Align) {
        self.reveal.set(self.selection.get_untracked().row.map(|row| (row, align)));
    }

    fn split_when_wide(self) {
        let options = self.with(ReviewController::options);
        let status = self.status;
        let collapsed = memo(move || status.with(|status| status.sidebar_collapsed));
        let wide = memo(move || {
            let width = self.width.get()?;
            let diff = if collapsed.get() { width } else { width - self.sidebar_width.get() };
            Some(diff >= options.auto_split_width)
        });
        effect(move || {
            if let Some(wide) = wide.get() {
                self.update(|state| state.session.set_split_when_auto(wide));
            }
        });
    }

    fn prepare_syntax(self) {
        let document = self.state.read_only().select(|state| SyntaxDocument(state.syntax.clone()));
        effect(move || {
            let SyntaxDocument(syntax) = document.get();
            spawn(syntax_job::prepare(syntax, self.syntax, self.focus_file));
        });
    }

    fn update<T>(&self, f: impl FnOnce(&mut ReviewController) -> T) -> Option<T> {
        if !self.is_alive() {
            return None;
        }
        batch(|| {
            let anchor = self.viewport_anchor();
            let (result, moved, effects) = self.state.try_update(|state| {
                let projection = state.session.projection_revision();
                let previous_file = state.session.selected_file();
                let annotations = state.session.annotations_revision();
                let result = f(state);
                state.reconcile(previous_file, annotations);
                (result, projection != state.session.projection_revision(), mem::take(&mut state.effects))
            })?;
            if moved && let Some(anchor) = anchor {
                self.restore_viewport(anchor);
            }
            for effect in effects {
                self.apply(effect);
            }
            Some(result)
        })
    }

    fn viewport_anchor(&self) -> Option<(VirtualListHandle, ViewportAnchor, f32)> {
        let list = self.viewport.get()?.diff;
        untrack(|| {
            let scroll = list.scroll_state().try_get()?;
            if scroll.viewport.height == 0.0 {
                return None;
            }
            let top = list.top_row().get();
            let anchor = self.with(|state| {
                ViewportAnchor::capture(state.session.presentation(), state.session.visible_range().start + top)
            })?;
            Some((list, anchor, scroll.offset.y - list.row_top(top)))
        })
    }

    fn restore_viewport(&self, (list, anchor, offset): (VirtualListHandle, ViewportAnchor, f32)) {
        self.with(|state| {
            let range = state.session.visible_range();
            if let Some(row) = anchor.resolve(state.session.presentation(), range.clone()) {
                list.scroll_to_row_offset(row - range.start, offset);
            }
        });
    }

    fn apply(&self, effect: Effect) {
        match effect {
            Effect::Output(output) => self.output.call(output),
            Effect::Scroll(request) => {
                if let Some(viewport) = self.viewport.get() {
                    viewport.scroll(request);
                }
            }
            Effect::Picker(command) => {
                if let Some(picker) = self.picker.get() {
                    command.apply(&picker);
                }
            }
        }
    }

    fn page(&self, command: DiffReviewCommand) -> DiffReviewCommand {
        let DiffReviewCommand::Page(delta) = command else {
            return command;
        };
        let pane = self.with(|state| state.pane);
        let rows = self.viewport.get().map_or(1, |viewport| viewport.page_rows(pane));
        DiffReviewCommand::MoveSelection(delta.saturating_mul(rows))
    }

    pub fn handle_command(&self, command: impl Into<DiffReviewCommand>) -> bool {
        use clankerdiff_core::{DiffReviewCommand as C, ReviewCommand as R};
        if !self.is_alive() {
            return false;
        }
        let command = self.page(command.into());
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
        if self.theme.try_get().is_some_and(|current| current.revision() != theme.revision()) {
            self.theme.set(theme);
        }
    }

    pub fn set_background_error(&self, error: Option<String>) {
        self.update(|state| state.background_error = error);
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
        self.state.try_update(|state| {
            if state.session.draft().is_some_and(|draft| draft.body() != body)
                && let Some(draft) = state.session.draft_mut()
            {
                draft.set_body(body);
            }
        });
    }
}

#[derive(Clone)]
struct SyntaxDocument(Arc<DocumentSyntax>);

impl PartialEq for SyntaxDocument {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[must_use]
pub fn diff_view(handle: ReviewHandle) -> impl View {
    diff_view_with(handle, move || content(handle))
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
        effect(move || {
            let width = bounds.get().size.width / font_size.get();
            if width.is_finite() {
                handle.width.set_if_changed(Some(width));
            }
        });
        let active = ActiveReviewTheme::install(handle.theme.get_untracked());
        effect({
            let theme = handle.theme();
            move || active.set(theme.get())
        });
        let shortcuts = Shortcuts::new();
        let focus = focus_handle();
        let was_draft = Cell::new(false);
        effect({
            let overlay = handle.overlay;
            move || {
                let drafting = overlay.get() == OverlayKind::Draft;
                if was_draft.replace(drafting) && !drafting {
                    focus.focus();
                }
            }
        });
        view! {
            <div test_id="diff-viewer" size_full relative focusable track_focus=focus track_bounds=bounds autofocus
                 on_action={move |action: &RunCommand| {
                     handle.handle_command(action.0.clone());
                 }}
                 on_key_down={move |event| handle.key_down(event, &shortcuts)}>
                {content()}
                <dynamic>{overlay::view(&handle, handle.overlay.get())}</dynamic>
            </div>
        }
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
