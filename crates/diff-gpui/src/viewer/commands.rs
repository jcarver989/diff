use super::{DiffViewer, ViewerPane};
use crate::{DiffViewerEvent, ThemeChanged};
use clankerdiff_core::{
    CommandContext, DiffReviewCommand, InteractionPhase, RepositoryAction, ReviewCapabilities,
    ReviewCommand,
};
use clankerdiff_theme::{ThemeChoice, ThemeSelection};
use gpui::{Context, ListOffset, Window, px};

impl DiffViewer {
    pub fn command_context(&self) -> CommandContext {
        let phase = if self.theme_selection.is_some() {
            InteractionPhase::ThemePicker
        } else if self.repository_prompt.is_some() {
            InteractionPhase::RepositoryPrompt
        } else if self.shortcuts_open {
            InteractionPhase::Help
        } else if self.session.draft().is_some() {
            InteractionPhase::Draft
        } else {
            InteractionPhase::Browse
        };
        CommandContext {
            phase,
            capabilities: self.capabilities,
            repository_pending: self.repository_pending,
            themes_available: true,
            ..CommandContext::default()
        }
    }

    pub fn command_enabled(&self, command: &DiffReviewCommand) -> bool {
        command.enabled(&self.command_context())
            && !matches!(
                command,
                DiffReviewCommand::Review(ReviewCommand::ScrollHelp(_))
            )
            && (!matches!(command, DiffReviewCommand::CopyReview) || !self.review().is_empty())
    }

    pub fn set_capabilities(&mut self, capabilities: ReviewCapabilities, cx: &mut Context<Self>) {
        self.capabilities = capabilities;
        if !capabilities.repository {
            self.clear_repository_prompt();
        }
        cx.notify();
    }

    pub fn handle_command(
        &mut self,
        command: impl Into<DiffReviewCommand>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use DiffReviewCommand as C;
        use ReviewCommand as R;
        let command = command.into();
        if !self.command_enabled(&command) {
            return false;
        }
        match command {
            C::Focus(pane) => {
                if self.pane == pane {
                    return true;
                }
                self.pane = pane;
            }
            C::ToggleFocus => {
                self.pane = match self.pane {
                    ViewerPane::Files => ViewerPane::Diff,
                    ViewerPane::Diff => ViewerPane::Files,
                }
            }
            C::SelectFile(index) => {
                if index >= self.document().files.len() {
                    return false;
                }
                self.select_file(index, cx);
                self.reveal_sidebar_selection();
                return true;
            }
            C::SelectRow(index) => {
                let previous = (self.session.selected_row(), self.session.selected_side());
                if !self
                    .session
                    .selected_file_range()
                    .is_some_and(|range| range.contains(&index))
                    || !self.session.select_row(index)
                {
                    return false;
                }
                if previous == (self.session.selected_row(), self.session.selected_side()) {
                    return true;
                }
                self.reveal_selected_row();
            }
            C::SelectSide(side) => {
                let previous = self.session.selected_side();
                self.session.set_selected_side(side);
                if previous == self.session.selected_side() {
                    return true;
                }
                self.reveal_selected_row();
            }
            C::MoveSelection(delta) => {
                self.move_item(delta, cx);
                return true;
            }
            C::Page(delta) => {
                self.page_items(delta, cx);
                return true;
            }
            C::First | C::Last => {
                self.select_boundary(command == C::Last, cx);
                return true;
            }
            C::OpenSelected => {
                self.open_selected_entry(cx);
                return true;
            }
            C::CollapseSelected => {
                self.collapse_selected_entry(cx);
                return true;
            }
            C::RevealGap(amount) => {
                self.expand_selected_gap(amount, cx);
                return true;
            }
            C::ToggleFullFile => {
                let changed = self.session.toggle_full_file();
                self.finish_layout_change(changed, cx);
                return true;
            }
            C::SetViewMode(mode) => {
                let changed = self.session.set_view_mode(mode);
                self.finish_layout_change(changed, cx);
                return true;
            }
            C::CycleViewMode => {
                let changed = self.session.cycle_view_mode();
                self.finish_layout_change(changed, cx);
                return true;
            }
            C::ToggleStage => {
                self.toggle_stage_entry(self.sidebar_selection.clone(), window, cx);
                return true;
            }
            C::BeginCommit => {
                self.open_commit_prompt(window, cx);
                return true;
            }
            C::BeginDiscard => {
                self.open_discard_prompt(cx);
                return true;
            }
            C::StageAll => {
                cx.emit(DiffViewerEvent::RepositoryAction(
                    RepositoryAction::StageAll,
                ));
                return true;
            }
            C::UnstageAll => {
                cx.emit(DiffViewerEvent::RepositoryAction(
                    RepositoryAction::UnstageAll,
                ));
                return true;
            }
            C::RepositoryAction(action) => {
                cx.emit(DiffViewerEvent::RepositoryAction(action));
                return true;
            }
            C::SetScope(scope) => {
                cx.emit(DiffViewerEvent::SetScope(scope));
                return true;
            }
            C::CycleScope => {
                cx.emit(DiffViewerEvent::SetScope(self.scope.next()));
                return true;
            }
            C::Refresh => {
                cx.emit(DiffViewerEvent::Refresh);
                return true;
            }
            C::SubmitReview => {
                cx.emit(DiffViewerEvent::SubmitReview(self.session.submission()));
                return true;
            }
            C::CopyReview => {
                cx.emit(DiffViewerEvent::CopyFormattedReview(
                    self.session.submission().formatted,
                ));
                return true;
            }
            C::Scroll {
                pane: ViewerPane::Files,
                lines,
            } => self.sidebar_scroll_handle.scroll_to_item(
                self.sidebar_scroll_handle
                    .top_item()
                    .saturating_add_signed(lines),
            ),
            C::Scroll {
                pane: ViewerPane::Diff,
                lines,
            } => {
                let top = self.diff_list_state.logical_scroll_top();
                self.diff_list_state.scroll_to(ListOffset {
                    item_ix: top.item_ix.saturating_add_signed(lines),
                    offset_in_item: px(0.0),
                });
            }
            C::Review(R::BeginComment | R::EditComment) => {
                let started = if command == C::Review(R::BeginComment) {
                    self.session.begin_draft(None)
                } else {
                    self.session.edit_comment_at_selection()
                };
                if !started {
                    return false;
                }
                self.mount_comment_editor(window, cx);
                self.reveal_selected_row();
                return true;
            }
            C::Review(R::DeleteComment | R::UndoComment) => {
                let changed = if command == C::Review(R::DeleteComment) {
                    self.session.delete_comment_at_selection()
                } else {
                    self.session.undo_last_comment()
                };
                if !changed {
                    return false;
                }
                self.diff_list_state.remeasure();
            }
            C::Review(R::SubmitComment) => {
                self.finish_comment(cx);
                return true;
            }
            C::Review(R::Cancel) => match self.command_context().phase {
                InteractionPhase::Browse => {
                    cx.emit(DiffViewerEvent::Cancel);
                    return true;
                }
                InteractionPhase::Draft => {
                    self.discard_comment(cx);
                    return true;
                }
                InteractionPhase::ThemePicker => {
                    if let Some(selection) = self.theme_selection.take() {
                        self.apply_theme(selection.cancel(), cx);
                    }
                    return true;
                }
                InteractionPhase::RepositoryPrompt => self.clear_repository_prompt(),
                InteractionPhase::Help => self.shortcuts_open = false,
            },
            C::Review(R::ShowHelp) => self.shortcuts_open = true,
            C::Review(R::ScrollHelp(_)) => return false,
            C::Review(R::OpenThemePicker) => {
                self.theme_selection = ThemeSelection::new(&self.theme, ThemeChoice::catalog())
            }
            C::Review(R::SelectTheme(index)) => {
                let Some(theme) = self
                    .theme_selection
                    .as_mut()
                    .and_then(|selection| selection.select(index))
                else {
                    return false;
                };
                self.apply_theme(theme, cx);
                return true;
            }
            C::Review(R::MoveTheme(delta)) => {
                let Some(selection) = self.theme_selection.as_mut() else {
                    return false;
                };
                let theme = selection.select_relative(delta);
                self.apply_theme(theme, cx);
                return true;
            }
            C::Review(R::CommitTheme) => {
                let Some(selection) = self.theme_selection.take() else {
                    return false;
                };
                let theme = selection.commit();
                let id = theme.id().to_string();
                self.apply_theme(theme, cx);
                cx.emit(ThemeChanged { id });
                return true;
            }
        }
        cx.notify();
        true
    }
}
