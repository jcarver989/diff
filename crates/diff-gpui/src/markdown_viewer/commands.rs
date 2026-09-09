use super::MarkdownReviewer;
use crate::ThemeChanged;
use clankerdiff_core::{CommandContext, InteractionPhase, ReviewCapabilities, ReviewCommand};
use clankerdiff_markdown::{
    MarkdownFocusPane, MarkdownReviewCommand, MarkdownReviewError, MarkdownReviewEvent,
};
use clankerdiff_theme::{ThemeChoice, ThemeSelection};
use gpui::{Context, Window};

impl MarkdownReviewer {
    pub fn command_context(&self) -> CommandContext {
        CommandContext {
            phase: if self.theme_selection.is_some() {
                InteractionPhase::ThemePicker
            } else if self.session.draft().is_some() {
                InteractionPhase::Draft
            } else {
                InteractionPhase::Browse
            },
            capabilities: self.capabilities,
            navigation_available: self.options.show_outline,
            themes_available: true,
            ..CommandContext::default()
        }
    }

    pub fn command_enabled(&self, command: &MarkdownReviewCommand) -> bool {
        !matches!(
            command,
            MarkdownReviewCommand::Review(ReviewCommand::ShowHelp | ReviewCommand::ScrollHelp(_))
        ) && command.enabled(&self.command_context())
    }

    pub fn set_capabilities(&mut self, capabilities: ReviewCapabilities, cx: &mut Context<Self>) {
        self.capabilities = capabilities;
        cx.notify();
    }

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
                let scroll = match self.pane {
                    MarkdownFocusPane::Document => &self.document_scroll,
                    MarkdownFocusPane::Outline => &self.outline_scroll,
                };
                let rows = scroll
                    .bottom_item()
                    .saturating_sub(scroll.top_item())
                    .saturating_add(1);
                self.move_selection(
                    delta.saturating_mul(isize::try_from(rows).unwrap_or(isize::MAX)),
                );
            }
            C::OpenSelected => {
                self.pane = MarkdownFocusPane::Document;
                self.reveal_selected_target();
            }
            C::Scroll { pane, lines } => {
                let scroll = match pane {
                    MarkdownFocusPane::Document => &self.document_scroll,
                    MarkdownFocusPane::Outline => &self.outline_scroll,
                };
                scroll.scroll_to_item(scroll.top_item().saturating_add_signed(lines));
                cx.notify();
                return Ok(true);
            }
            C::Approve => {
                cx.emit(self.session.approve()?);
                return Ok(true);
            }
            C::RequestChanges => {
                cx.emit(self.session.request_changes()?);
                return Ok(true);
            }
            C::CopyReview(decision) => {
                cx.emit(self.session.copy_formatted(decision));
                return Ok(true);
            }
            C::Review(R::BeginComment | R::EditComment) => {
                let started = if command == C::Review(R::BeginComment) {
                    self.session.begin_draft(None)
                } else {
                    self.session.edit_comment_at_selection()
                };
                if !started {
                    return Ok(false);
                }
                self.mount_editor(window, cx);
                self.reveal_selected_target();
                return Ok(true);
            }
            C::Review(R::DeleteComment | R::UndoComment) => {
                let changed = if command == C::Review(R::DeleteComment) {
                    self.session.delete_comment_at_selection()
                } else {
                    self.session.undo_last_comment()
                };
                if changed {
                    cx.notify();
                }
                return Ok(changed);
            }
            C::Review(R::SubmitComment) => {
                self.finish_comment(cx);
                return Ok(true);
            }
            C::Review(R::Cancel) => {
                if let Some(selection) = self.theme_selection.take() {
                    self.apply_theme(selection.cancel(), cx);
                } else if self.session.draft().is_some() {
                    self.discard_comment(cx);
                } else {
                    cx.emit(MarkdownReviewEvent::Cancel);
                }
                return Ok(true);
            }
            C::Review(R::ShowHelp | R::ScrollHelp(_)) => return Ok(false),
            C::Review(R::OpenThemePicker) => {
                self.theme_selection = ThemeSelection::new(&self.theme, ThemeChoice::catalog());
                cx.notify();
                return Ok(true);
            }
            C::Review(R::SelectTheme(index)) => {
                let Some(theme) = self
                    .theme_selection
                    .as_mut()
                    .and_then(|selection| selection.select(index))
                else {
                    return Ok(false);
                };
                self.apply_theme(theme, cx);
                return Ok(true);
            }
            C::Review(R::MoveTheme(delta)) => {
                let Some(selection) = self.theme_selection.as_mut() else {
                    return Ok(false);
                };
                let theme = selection.select_relative(delta);
                self.apply_theme(theme, cx);
                return Ok(true);
            }
            C::Review(R::CommitTheme) => {
                let Some(selection) = self.theme_selection.take() else {
                    return Ok(false);
                };
                let theme = selection.commit();
                let id = theme.id().to_string();
                self.apply_theme(theme, cx);
                cx.emit(ThemeChanged { id });
                return Ok(true);
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

    pub(super) fn reveal_selected_target(&self) {
        if let Some(target) = self.session.selected_target() {
            self.document_scroll.scroll_to_item(target.index());
        }
    }

    fn move_selection(&mut self, delta: isize) {
        match self.pane {
            MarkdownFocusPane::Document => {
                self.session.move_target(delta);
            }
            MarkdownFocusPane::Outline => {
                let outline = self.document().outline();
                let selected = self
                    .session
                    .selected_target()
                    .map_or(0, |target| target.index());
                let current = outline
                    .partition_point(|heading| heading.target_id.index() <= selected)
                    .saturating_sub(1);
                let index = current
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
