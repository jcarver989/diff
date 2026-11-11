mod chrome;
mod content;
mod document;

use crate::ui::{
    InterfaceSize,
    interface_size::DEFAULT_INTERFACE_SIZE,
    picker::{SearchPickerHandle, ThemePickerEvent},
    prelude::*,
};
use clankerdiff_core::{CommandContext, InteractionPhase, ReviewCapabilities, ReviewCommand};
use clankerdiff_markdown::{
    MarkdownDocument, MarkdownFocusPane, MarkdownReviewCommand as Command, MarkdownReviewDecision, MarkdownReviewError,
    MarkdownReviewEvent, MarkdownReviewSession, MarkdownTargetId,
};
use clankerdiff_ui_core::{ReviewTheme, ThemeChoice};
use content::Paint;
use rouge_core::{
    Align, AnyView, Key, KeyEvent, Modifiers, Styled, View, VirtualListHandle, div, dynamic, focus_handle, overlay,
    virtual_list_handle,
};
use rouge_reactive::{Signal, batch, effect, signal, untrack, use_context};
use std::{rc::Rc, sync::Arc};

const DEFAULT_OUTLINE_WIDTH: f32 = 220.0 / 16.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownOutput {
    Event(MarkdownReviewEvent),
    ThemeChanged(String),
    CopyCode(String),
    OpenLink(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Overlay {
    None,
    Help,
    Themes,
}

#[derive(Clone)]
pub struct MarkdownHandle {
    session: Signal<MarkdownReviewSession>,
    rows: Signal<Rc<content::Rows>>,
    revision: Signal<u64>,
    selected: Signal<Option<MarkdownTargetId>>,
    pane: Signal<MarkdownFocusPane>,
    draft: Signal<Option<(MarkdownTargetId, Option<u64>)>>,
    annotations: Signal<u64>,
    theme: Signal<ReviewTheme>,
    capabilities: Signal<ReviewCapabilities>,
    overlay: Signal<Overlay>,
    picker: Signal<Option<SearchPickerHandle>>,
    outline_collapsed: Signal<bool>,
    outline_width: Signal<f32>,
    list: VirtualListHandle,
    outline: VirtualListHandle,
    emit: Rc<dyn Fn(MarkdownOutput)>,
}

pub struct MarkdownController {
    session: MarkdownReviewSession,
    theme: ReviewTheme,
}

impl MarkdownController {
    #[must_use]
    pub fn new(document: Arc<MarkdownDocument>, theme: ReviewTheme) -> Self {
        Self { session: MarkdownReviewSession::new(document), theme }
    }

    #[must_use]
    pub fn mount(self, emit: impl Fn(MarkdownOutput) + 'static) -> MarkdownHandle {
        MarkdownHandle {
            rows: signal(Rc::new(content::Rows::new(self.session.document().clone()))),
            selected: signal(self.session.selected_target()),
            session: signal(self.session),
            revision: signal(0),
            pane: signal(MarkdownFocusPane::Document),
            draft: signal(None),
            annotations: signal(0),
            theme: signal(self.theme),
            capabilities: signal(ReviewCapabilities::default()),
            overlay: signal(Overlay::None),
            picker: signal(None),
            outline_collapsed: signal(false),
            outline_width: signal(DEFAULT_OUTLINE_WIDTH),
            list: virtual_list_handle(),
            outline: virtual_list_handle(),
            emit: Rc::new(emit),
        }
    }
}

impl MarkdownHandle {
    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.session.is_alive()
    }

    pub fn with<T>(&self, f: impl FnOnce(&MarkdownReviewSession) -> T) -> T {
        self.session.with_untracked(f)
    }

    pub fn set_document(&self, document: Arc<MarkdownDocument>) {
        if !self.is_alive() || self.with(|session| session.document().as_ref() == document.as_ref()) {
            return;
        }
        batch(|| {
            self.rows.set(Rc::new(content::Rows::new(document.clone())));
            self.revision.update(|revision| *revision = revision.wrapping_add(1));
            self.edit(|session| session.replace_document(document));
        });
        self.reveal();
    }

    pub fn set_capabilities(&self, caps: ReviewCapabilities) {
        self.capabilities.try_update(|value| *value = caps);
    }

    pub fn set_theme(&self, theme: ReviewTheme) {
        self.theme.try_update(|value| *value = theme);
    }

    pub fn clear_review(&self) {
        if self.is_alive() {
            self.edit(MarkdownReviewSession::clear_review);
        }
    }

    #[must_use]
    pub fn command_context(&self) -> CommandContext {
        CommandContext {
            phase: match self.overlay.get_untracked() {
                Overlay::Help => InteractionPhase::Help,
                Overlay::Themes => InteractionPhase::ThemePicker,
                Overlay::None if self.draft.get_untracked().is_some() => InteractionPhase::Draft,
                Overlay::None => InteractionPhase::Browse,
            },
            navigation_available: self.outline_visible_untracked(),
            capabilities: self.capabilities.get_untracked(),
            themes_available: true,
            ..CommandContext::default()
        }
    }

    fn command_enabled(&self, command: Command) -> bool {
        !matches!(command, Command::Review(ReviewCommand::ScrollHelp(_))) && command.enabled(&self.command_context())
    }

    fn outline_available(&self) -> bool {
        self.rows.with(|rows| rows.document.outline().len() > 1)
    }

    fn outline_visible(&self) -> bool {
        self.outline_available() && !self.outline_collapsed.get()
    }

    fn outline_visible_untracked(&self) -> bool {
        untrack(|| self.outline_visible())
    }

    fn toggle_outline(&self) {
        if !self.outline_available() {
            return;
        }
        batch(|| {
            self.outline_collapsed.update(|collapsed| *collapsed = !*collapsed);
            if self.outline_collapsed.get_untracked() {
                self.pane.set_if_changed(MarkdownFocusPane::Document);
            }
        });
    }

    fn edit<T>(&self, f: impl FnOnce(&mut MarkdownReviewSession) -> T) -> T {
        batch(|| {
            let result = self.session.try_update(f).expect("mounted Markdown review");
            self.with(|session| {
                self.selected.set_if_changed(session.selected_target());
                self.draft.set_if_changed(session.draft().map(|d| (d.target(), d.editing())));
                self.annotations.set_if_changed(session.annotations_revision());
            });
            result
        })
    }

    fn reveal(&self) {
        let Some(target) = self.selected.get_untracked() else { return };
        if let Some(row) = self.rows.with_untracked(|rows| rows.index(target)) {
            self.list.scroll_to_row(row, Align::Visible);
        }
        if let Some(heading) = self.active_heading() {
            self.outline.scroll_to_row(heading, Align::Visible);
        }
    }

    fn active_heading(&self) -> Option<usize> {
        let target = self.selected.get_untracked()?;
        self.with(|session| session.document().outline().partition_point(|h| h.target_id <= target).checked_sub(1))
    }

    fn copy_decision(&self) -> MarkdownReviewDecision {
        if self.with(|session| session.review().is_empty()) {
            MarkdownReviewDecision::Approved
        } else {
            MarkdownReviewDecision::ChangesRequested
        }
    }

    pub fn handle_command(&self, command: impl Into<Command>) -> Result<bool, MarkdownReviewError> {
        use Command as C;
        let command = command.into();
        if !self.is_alive() || !self.command_enabled(command) {
            return Ok(false);
        }
        let selected = self.selected.get_untracked();
        let (handled, event) = match command {
            C::Approve => (true, Some(self.with(MarkdownReviewSession::approve)?)),
            C::RequestChanges => (true, Some(self.with(MarkdownReviewSession::request_changes)?)),
            C::CopyReview(decision) => (true, Some(self.with(|s| s.copy_formatted(decision)))),
            C::Review(review) => (self.review_command(review), None),
            other => (self.navigate(other), None),
        };
        if selected != self.selected.get_untracked() {
            self.reveal();
        }
        if let Some(event) = event {
            (self.emit)(MarkdownOutput::Event(event));
        }
        Ok(handled)
    }

    fn review_command(&self, command: ReviewCommand) -> bool {
        use ReviewCommand as R;
        match command {
            R::Cancel => match self.overlay.get_untracked() {
                Overlay::Themes => {
                    if let Some(picker) = self.picker.get_untracked() {
                        picker.dismiss();
                    }
                }
                Overlay::Help => self.overlay.set(Overlay::None),
                Overlay::None if self.draft.get_untracked().is_some() => {
                    self.edit(MarkdownReviewSession::cancel_draft);
                }
                Overlay::None => (self.emit)(MarkdownOutput::Event(MarkdownReviewEvent::Cancel)),
            },
            R::ShowHelp => self.overlay.set(Overlay::Help),
            R::OpenThemePicker => self.overlay.set(Overlay::Themes),
            R::SelectTheme(_) | R::MoveTheme(_) | R::CommitTheme => {
                let Some(picker) = self.picker.get_untracked() else { return false };
                match command {
                    R::SelectTheme(i) => picker.highlight(i),
                    R::MoveTheme(n) => picker.move_highlight(n),
                    _ => picker.choose(),
                }
            }
            R::ScrollHelp(_) => return false,
            R::BeginComment | R::EditComment | R::DeleteComment | R::UndoComment | R::SubmitComment => {
                let changed = self.edit(|s| match command {
                    R::BeginComment => s.begin_draft(None),
                    R::EditComment => s.edit_comment_at_selection(),
                    R::DeleteComment => s.delete_comment_at_selection(),
                    R::UndoComment => s.undo_last_comment(),
                    _ => {
                        s.submit_draft();
                        true
                    }
                });
                if !changed {
                    return false;
                }
                self.reveal();
            }
        }
        true
    }

    fn navigate(&self, command: Command) -> bool {
        use Command as C;
        match command {
            C::Focus(pane) => {
                self.pane.set_if_changed(pane);
            }
            C::ToggleFocus => self.pane.update(|pane| {
                *pane = match pane {
                    MarkdownFocusPane::Document => MarkdownFocusPane::Outline,
                    MarkdownFocusPane::Outline => MarkdownFocusPane::Document,
                }
            }),
            C::OpenSelected => {
                self.pane.set_if_changed(MarkdownFocusPane::Document);
                self.reveal();
            }
            C::Scroll { pane, lines } => {
                let list = self.list_for(pane);
                list.scroll_to_row(list.top_row().get_untracked().saturating_add_signed(lines), Align::Top);
            }
            C::SelectTarget(id) => return self.edit(|s| s.select_target(id)),
            C::SelectHeading(index) => {
                let target = self.with(|s| s.document().outline().get(index).map(|heading| heading.target_id));
                let Some(target) = target else { return false };
                self.edit(|s| s.select_target(target));
            }
            C::MoveSelection(delta) => self.move_selection(delta),
            C::First => self.move_selection(isize::MIN),
            C::Last => self.move_selection(isize::MAX),
            C::Page(delta) => self.move_selection(delta.saturating_mul(self.page_rows())),
            C::NextHeading => {
                self.edit(MarkdownReviewSession::next_heading);
            }
            C::PreviousHeading => {
                self.edit(MarkdownReviewSession::previous_heading);
            }
            C::Review(_) | C::Approve | C::RequestChanges | C::CopyReview(_) => return false,
        }
        true
    }

    const fn list_for(&self, pane: MarkdownFocusPane) -> VirtualListHandle {
        match pane {
            MarkdownFocusPane::Document => self.list,
            MarkdownFocusPane::Outline => self.outline,
        }
    }

    fn page_rows(&self) -> isize {
        let pane = self.pane.get_untracked();
        let list = self.list_for(pane);
        let count = match pane {
            MarkdownFocusPane::Document => self.rows.with_untracked(|rows| rows.len()),
            MarkdownFocusPane::Outline => self.with(|s| s.document().outline().len()),
        };
        let state = list.scroll_state().get_untracked();
        let bottom = state.offset.y + state.viewport.height;
        let visible = (list.top_row().get_untracked()..count).take_while(|row| list.row_top(*row) < bottom).count();
        isize::try_from(visible.max(1)).unwrap_or(1)
    }

    fn move_selection(&self, delta: isize) {
        match self.pane.get_untracked() {
            MarkdownFocusPane::Document => {
                self.edit(|session| session.move_target(delta));
            }
            MarkdownFocusPane::Outline => {
                let count = self.with(|s| s.document().outline().len());
                let index =
                    self.active_heading().unwrap_or(0).saturating_add_signed(delta).min(count.saturating_sub(1));
                let target = self.with(|s| s.document().outline().get(index).map(|heading| heading.target_id));
                if let Some(target) = target {
                    self.edit(|session| session.select_target(target));
                    self.outline.scroll_to_row(index, Align::Visible);
                }
            }
        }
    }

    fn select(&self, target: MarkdownTargetId, pane: MarkdownFocusPane) {
        self.run(Command::Focus(pane));
        self.run(Command::SelectTarget(target));
    }

    fn begin_comment_on(&self, target: MarkdownTargetId) {
        self.select(target, MarkdownFocusPane::Document);
        self.run(ReviewCommand::BeginComment);
    }

    fn edit_comment(&self, target: MarkdownTargetId, id: u64) {
        if self.command_context().phase != InteractionPhase::Browse {
            return;
        }
        self.select(target, MarkdownFocusPane::Document);
        if self.edit(|session| session.begin_draft(Some(id))) {
            self.reveal();
        }
    }

    fn delete_comment(&self, id: u64) {
        self.edit(|session| {
            if session.draft().is_some_and(|draft| draft.editing() == Some(id)) {
                session.cancel_draft();
            }
            session.review_mut().remove_comment(id);
        });
    }

    fn set_draft_body(&self, body: String) {
        self.session.try_update(|session| {
            if let Some(draft) = session.draft_mut() {
                draft.set_body(body);
            }
        });
    }

    fn run(&self, command: impl Into<Command>) {
        let _ = self.handle_command(command);
    }

    fn key_command(&self, event: &KeyEvent) -> Option<Command> {
        use Command as C;
        use ReviewCommand as R;
        let modifiers = event.modifiers;
        let character = match &event.key {
            Key::Character(key) => Some(key.as_str()),
            _ => None,
        };
        if event.key == Key::Escape {
            return Some(R::Cancel.into());
        }
        if self.command_context().phase != InteractionPhase::Browse
            || modifiers.alt
            || modifiers.ctrl && !modifiers.command()
        {
            return None;
        }
        if modifiers.command() {
            return match character.map(str::to_ascii_lowercase).as_deref() {
                Some("c") if modifiers.shift => Some(C::CopyReview(self.copy_decision())),
                Some("t") if modifiers.shift => Some(R::OpenThemePicker.into()),
                _ => None,
            };
        }
        let help = event.text.as_deref() == Some("?") || character == Some("/") && modifiers.shift;
        if help {
            return Some(R::ShowHelp.into());
        }
        match &event.key {
            Key::ArrowDown => Some(C::MoveSelection(1)),
            Key::ArrowUp => Some(C::MoveSelection(-1)),
            Key::PageDown => Some(C::Page(1)),
            Key::PageUp => Some(C::Page(-1)),
            Key::Home => Some(C::First),
            Key::End => Some(C::Last),
            Key::Tab => Some(C::ToggleFocus),
            Key::Enter => Some(C::OpenSelected),
            Key::Character(key) => match key.as_str() {
                "j" => Some(C::MoveSelection(1)),
                "k" => Some(C::MoveSelection(-1)),
                "g" => Some(C::First),
                "G" => Some(C::Last),
                "n" => Some(C::NextHeading),
                "p" => Some(C::PreviousHeading),
                "c" => Some(R::BeginComment.into()),
                "e" => Some(R::EditComment.into()),
                "x" => Some(R::DeleteComment.into()),
                "u" => Some(R::UndoComment.into()),
                "a" => Some(C::Approve),
                "r" => Some(C::RequestChanges),
                "y" => Some(C::CopyReview(self.copy_decision())),
                "t" => Some(R::OpenThemePicker.into()),
                _ => None,
            },
            _ => None,
        }
    }
}

#[must_use]
pub fn markdown_review_view(handle: MarkdownHandle) -> impl View {
    let active = ActiveTheme::install(handle.theme.get_untracked());
    let theme_signal = handle.theme;
    effect(move || active.set(theme_signal.get()));
    let theme = active.ui();
    let size = use_context::<InterfaceSize>().unwrap_or_else(|| InterfaceSize::install(DEFAULT_INTERFACE_SIZE));
    let focus = focus_handle();
    let keys = handle.clone();
    let paint =
        Paint { ui: theme, palette: handle.theme, capabilities: handle.capabilities, emit: handle.emit.clone() };
    let body = div()
        .flex()
        .row()
        .flex_1()
        .min_h(0.0)
        .child(dynamic({
            let handle = handle.clone();
            move || {
                if handle.outline_visible() { chrome::outline(&handle, theme).into_any() } else { div().into_any() }
            }
        }))
        .child(document::document(&handle, paint));
    let root = div()
        .test_id("markdown-reviewer")
        .flex()
        .col()
        .size_full()
        .min_h(0.0)
        .font(UI_FONT)
        .bg(move || theme_signal.with(|t| color(t.diff.background)))
        .text_color(move || theme_signal.with(|t| color(t.diff.foreground)))
        .track_focus(focus)
        .autofocus()
        .on_key_down(move |event| {
            if matches!(&event.key, Key::Character(key) if key == "b")
                && event.modifiers == Modifiers::NONE
                && keys.command_context().phase == InteractionPhase::Browse
            {
                keys.toggle_outline();
                event.stop_propagation();
                return;
            }
            if keys.key_command(event).is_some_and(|command| keys.handle_command(command).unwrap_or(false)) {
                event.stop_propagation();
            }
        })
        .child(chrome::header(&handle, theme))
        .child(body)
        .child(overlay(move || dynamic(move || overlay_view(&handle, theme))));
    size.bind(root)
}

fn overlay_view(handle: &MarkdownHandle, theme: UiTheme) -> AnyView {
    match handle.overlay.get() {
        Overlay::None => div().into_any(),
        Overlay::Themes => {
            let current = handle.theme.get_untracked();
            let view = handle.clone();
            let choices = ThemeChoice::catalog();
            let picker = ThemePickerOverlay::new(
                ThemePicker::new("markdown-theme-picker", &current, choices, theme, move |event| match event {
                    ThemePickerEvent::Preview(theme) => view.set_theme(theme),
                    ThemePickerEvent::Commit(theme) => {
                        let id = theme.id().to_string();
                        view.set_theme(theme);
                        view.overlay.set(Overlay::None);
                        (view.emit)(MarkdownOutput::ThemeChanged(id));
                    }
                    ThemePickerEvent::Cancel(theme) => {
                        view.set_theme(theme);
                        view.overlay.set(Overlay::None);
                    }
                }),
                theme,
            );
            untrack(|| handle.picker.set(Some(picker.picker())));
            picker.into_any()
        }
        Overlay::Help => {
            let close = handle.clone();
            chrome::shortcuts(theme, move || close.run(ReviewCommand::Cancel)).into_any()
        }
    }
}
