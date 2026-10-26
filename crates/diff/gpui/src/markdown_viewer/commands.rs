use super::{MarkdownReviewer, Overlay};
use crate::ui::prelude::ThemePicker;
use clankerdiff_core::{CommandContext, InteractionPhase, ReviewCapabilities, ReviewCommand};
use clankerdiff_markdown::{
    MarkdownFocusPane, MarkdownReviewCommand, MarkdownReviewError, MarkdownReviewEvent, MarkdownTargetId,
};
use gpui::{App, Context, ScrollHandle, Window};

impl MarkdownReviewer {
    pub fn command_context(&self) -> CommandContext {
        CommandContext {
            phase: match self.overlay {
                Overlay::Themes { .. } => InteractionPhase::ThemePicker,
                Overlay::Help => InteractionPhase::Help,
                Overlay::None if self.session.draft().is_some() => InteractionPhase::Draft,
                Overlay::None => InteractionPhase::Browse,
            },
            capabilities: self.capabilities,
            navigation_available: self.outline_visible(),
            themes_available: self.options.themes,
            ..CommandContext::default()
        }
    }

    pub fn command_enabled(&self, command: &MarkdownReviewCommand) -> bool {
        !matches!(command, MarkdownReviewCommand::Review(ReviewCommand::ScrollHelp(_)))
            && command.enabled(&self.command_context())
    }

    pub fn set_capabilities(&mut self, capabilities: ReviewCapabilities, cx: &mut Context<Self>) {
        self.capabilities = capabilities;
        cx.notify();
    }

    #[expect(clippy::too_many_lines, reason = "Exhaustive command dispatch keeps routing in one place")]
    pub fn handle_command(
        &mut self,
        command: impl Into<MarkdownReviewCommand>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, MarkdownReviewError> {
        use MarkdownReviewCommand as C;
        use ReviewCommand as R;
        let command = command.into();
        if !self.command_enabled(&command) {
            return Ok(false);
        }
        let previous = (self.pane, self.session.selected_target());
        match command {
            C::Focus(pane) => self.pane = pane,
            C::ToggleFocus => {
                self.pane = match self.pane {
                    MarkdownFocusPane::Document => MarkdownFocusPane::Outline,
                    MarkdownFocusPane::Outline => MarkdownFocusPane::Document,
                }
            }
            C::SelectTarget(target) => {
                if !self.session.select_target(target) {
                    return Ok(false);
                }
            }
            C::SelectHeading(index) => {
                let Some(heading) = self.document().outline().get(index) else {
                    return Ok(false);
                };
                let target = heading.target_id;
                self.session.select_target(target);
            }
            C::NextHeading => {
                self.session.next_heading();
            }
            C::PreviousHeading => {
                self.session.previous_heading();
            }
            C::MoveSelection(delta) => self.move_selection(delta),
            C::First => self.move_selection(isize::MIN),
            C::Last => self.move_selection(isize::MAX),
            C::Page(delta) => {
                let scroll = self.scroll_handle(self.pane);
                let rows = scroll.bottom_item().saturating_sub(scroll.top_item()).saturating_add(1);
                self.move_selection(delta.saturating_mul(isize::try_from(rows).unwrap_or(isize::MAX)));
            }
            C::OpenSelected => {
                self.pane = MarkdownFocusPane::Document;
                self.reveal_selected_target();
            }
            C::Scroll { pane, lines } => {
                let scroll = self.scroll_handle(pane);
                scroll.scroll_to_item(scroll.top_item().saturating_add_signed(lines));
                cx.notify();
            }
            C::Approve => cx.emit(self.session.approve()?),
            C::RequestChanges => cx.emit(self.session.request_changes()?),
            C::CopyReview(decision) => cx.emit(self.session.copy_formatted(decision)),
            C::Review(R::BeginComment) => {
                if !self.session.begin_draft(None) {
                    return Ok(false);
                }
                self.mount_editor(window, cx);
            }
            C::Review(R::EditComment) => {
                if !self.session.edit_comment_at_selection() {
                    return Ok(false);
                }
                self.mount_editor(window, cx);
            }
            C::Review(R::DeleteComment) => {
                if !self.session.delete_comment_at_selection() {
                    return Ok(false);
                }
                cx.notify();
            }
            C::Review(R::UndoComment) => {
                if !self.session.undo_last_comment() {
                    return Ok(false);
                }
                cx.notify();
            }
            C::Review(R::SubmitComment) => {
                self.session.submit_draft();
                self.close_editor();
                self.reveal_selected_target();
                cx.notify();
            }
            C::Review(R::Cancel) => match self.overlay {
                Overlay::Themes { .. } => {
                    self.update_theme_picker(cx, ThemePicker::cancel);
                }
                Overlay::Help => {
                    self.overlay = Overlay::None;
                    cx.notify();
                }
                Overlay::None if self.session.draft().is_some() => self.discard_comment(cx),
                Overlay::None => cx.emit(MarkdownReviewEvent::Cancel),
            },
            C::Review(R::ShowHelp) => {
                self.overlay = Overlay::Help;
                cx.notify();
            }
            C::Review(R::ScrollHelp(_)) => return Ok(false),
            C::Review(R::OpenThemePicker) => self.open_theme_picker(window, cx),
            C::Review(R::SelectTheme(index)) => {
                return Ok(self.update_theme_picker(cx, |picker, cx| picker.highlight(index, cx)));
            }
            C::Review(R::MoveTheme(delta)) => {
                return Ok(self.update_theme_picker(cx, |picker, cx| picker.move_highlight(delta, cx)));
            }
            C::Review(R::CommitTheme) => {
                return Ok(self.update_theme_picker(cx, ThemePicker::commit));
            }
        }
        if previous.1 != self.session.selected_target() {
            self.close_editor();
            self.reveal_selected_target();
        }
        if previous != (self.pane, self.session.selected_target()) {
            cx.notify();
        }
        Ok(true)
    }

    pub(super) fn run(
        &mut self,
        command: impl Into<MarkdownReviewCommand>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.handle_command(command, window, cx);
    }

    pub(super) fn dispatch<E: 'static>(
        command: impl Into<MarkdownReviewCommand>,
        cx: &mut Context<Self>,
    ) -> impl Fn(&E, &mut Window, &mut App) + 'static {
        let command = command.into();
        cx.listener(move |reviewer, _: &E, window, cx| reviewer.run(command, window, cx))
    }

    pub(super) fn select(
        &mut self,
        target: MarkdownTargetId,
        pane: MarkdownFocusPane,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run(MarkdownReviewCommand::Focus(pane), window, cx);
        self.run(MarkdownReviewCommand::SelectTarget(target), window, cx);
    }

    pub(super) fn reveal_selected_target(&self) {
        if let Some(row) = self.session.selected_target().and_then(|target| self.target_rows.get(&target)) {
            self.document_scroll.scroll_to_item(*row);
        }
    }

    pub(super) fn active_heading(&self) -> Option<usize> {
        let selected = self.session.selected_target()?;
        self.document().outline().partition_point(|heading| heading.target_id <= selected).checked_sub(1)
    }

    const fn scroll_handle(&self, pane: MarkdownFocusPane) -> &ScrollHandle {
        match pane {
            MarkdownFocusPane::Document => &self.document_scroll,
            MarkdownFocusPane::Outline => &self.outline_scroll,
        }
    }

    fn move_selection(&mut self, delta: isize) {
        match self.pane {
            MarkdownFocusPane::Document => {
                self.session.move_target(delta);
            }
            MarkdownFocusPane::Outline => {
                let outline = self.document().outline();
                let index = self
                    .active_heading()
                    .unwrap_or(0)
                    .saturating_add_signed(delta)
                    .min(outline.len().saturating_sub(1));
                if let Some(heading) = outline.get(index) {
                    let target = heading.target_id;
                    self.session.select_target(target);
                    self.outline_scroll.scroll_to_item(index);
                }
            }
        }
    }
}
