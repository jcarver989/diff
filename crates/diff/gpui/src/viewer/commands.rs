use super::{DiffViewer, Overlay, ViewerPane};
use crate::{DiffViewerEvent, ui::prelude::ThemePicker};
use clankerdiff_core::{
    CommandContext, DiffReviewCommand, InteractionPhase, RepositoryAction, ReviewCapabilities, ReviewCommand,
    ReviewSession,
};
use gpui::{Context, ListOffset, ScrollStrategy, Window, px};

impl DiffViewer {
    #[must_use]
    pub fn command_context(&self) -> CommandContext {
        let phase = match &self.overlay {
            Some(overlay) => overlay.phase(),
            None if self.session.draft().is_some() => InteractionPhase::Draft,
            None => InteractionPhase::Browse,
        };
        CommandContext {
            phase,
            capabilities: self.capabilities,
            repository_pending: self.repository_pending,
            themes_available: self.options.themes,
            commits_available: self.session.commits().is_some(),
            source_view: self.session.shows_source(),
            ..CommandContext::default()
        }
    }

    #[must_use]
    pub fn command_enabled(&self, command: &DiffReviewCommand) -> bool {
        command.enabled(&self.command_context())
            && !matches!(command, DiffReviewCommand::Review(ReviewCommand::ScrollHelp(_)))
            && self.session.command_available(command)
    }

    pub fn set_capabilities(&mut self, capabilities: ReviewCapabilities, cx: &mut Context<Self>) {
        self.capabilities = capabilities;
        if !capabilities.repository {
            self.close_overlay(InteractionPhase::RepositoryPrompt);
        }
        cx.notify();
    }

    #[expect(clippy::too_many_lines, reason = "Exhaustive command dispatch keeps routing in one place")]
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
        if !command.keeps_definition_card() {
            self.close_names(cx);
        }
        match command {
            C::Focus(pane) => {
                if self.pane == pane {
                    return true;
                }
                self.pane = pane;
                self.reveal_focused_sidebar(cx);
            }
            C::ToggleFocus => {
                self.pane = match self.pane {
                    ViewerPane::Files => ViewerPane::Diff,
                    ViewerPane::Diff => ViewerPane::Files,
                };
                self.reveal_focused_sidebar(cx);
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
                if !self.session.select_row(index) {
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
            C::ToggleSourceView => {
                if self.session.source_view().is_none() && self.pane == ViewerPane::Files {
                    let Some(index) = self.sidebar_tree.selected().and_then(|entry| entry.file) else {
                        return false;
                    };
                    if self.session.selected_file() != Some(index) {
                        self.select_file(index, cx);
                    }
                }
                if !self.toggle_source_view(cx) {
                    return false;
                }
                self.pane = ViewerPane::Diff;
                return true;
            }
            C::ToggleFullFile => {
                self.change_layout(ReviewSession::toggle_full_file);
            }
            C::ToggleFileCollapsed(index) => {
                let Some(index) = index.or_else(|| self.session.selected_file()) else {
                    return false;
                };
                if !self.change_layout(|session| session.toggle_file_collapsed(index)) {
                    return false;
                }
            }
            C::ToggleAllFilesCollapsed => {
                if !self.change_layout(ReviewSession::toggle_all_files_collapsed) {
                    return false;
                }
            }
            C::SetViewMode(mode) => {
                self.change_view_mode(|session| session.set_view_mode(mode), cx);
            }
            C::ToggleLayout => {
                self.change_view_mode(ReviewSession::toggle_layout, cx);
            }
            C::ToggleStage => {
                if let Some(path) = self.sidebar_tree.selected().map(|entry| entry.path.clone()) {
                    self.toggle_stage_path(&path, window, cx);
                }
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
                cx.emit(DiffViewerEvent::RepositoryAction(RepositoryAction::StageAll));
                return true;
            }
            C::UnstageAll => {
                cx.emit(DiffViewerEvent::RepositoryAction(RepositoryAction::UnstageAll));
                return true;
            }
            C::RepositoryAction(action) => {
                cx.emit(DiffViewerEvent::RepositoryAction(action));
                return true;
            }
            C::ToggleSidebar => {
                let collapsed = self.sidebar_collapsed();
                self.set_sidebar_collapsed(!collapsed, cx);
                return true;
            }
            C::ShowDefinition => return self.show_definition_at_selection(cx),
            C::GoToDefinition => return self.names.card().is_some() && self.jump_to_card_choice(cx),
            C::NextDefinition => return self.cycle_card_choice(cx),
            C::CloseDefinition => return self.close_names(cx),
            command @ (C::JumpTo(_) | C::NavigateBack | C::NavigateForward) => return self.navigate(command, cx),
            command @ (C::SetScope(_) | C::CycleScope | C::StepTarget(_) | C::GoToPage(_)) => {
                let closed = self.close_overlay(InteractionPhase::CommitPicker).is_some();
                cx.notify();
                let Some(event) = self.session.target_event(&command) else {
                    return closed;
                };
                cx.emit(event);
                return true;
            }
            C::OpenCommitPicker => self.open_commit_picker(window, cx),
            C::MoveCommitPicker(delta) => {
                let Some(Overlay::Commits(overlay)) = &self.overlay else {
                    return false;
                };
                overlay.picker.update(cx, |picker, cx| picker.move_highlight(delta, cx));
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
                cx.emit(DiffViewerEvent::CopyFormattedReview(self.session.submission().formatted));
                return true;
            }
            C::Scroll { pane: ViewerPane::Files, lines } => self.sidebar_scroll_handle.scroll_to_item_strict(
                self.sidebar_rendered_rows.start.saturating_add_signed(lines),
                ScrollStrategy::Top,
            ),
            C::Scroll { pane: ViewerPane::Diff, lines } => {
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
                    return self.update_theme_picker(cx, ThemePicker::cancel);
                }
                InteractionPhase::RepositoryPrompt | InteractionPhase::CommitPicker | InteractionPhase::Help => {
                    self.overlay = None;
                }
            },
            C::Review(R::ShowHelp) => self.open_palette(window, cx),
            C::Review(R::ScrollHelp(_)) => return false,
            C::Review(R::OpenThemePicker) => self.open_theme_picker(window, cx),
            C::Review(R::SelectTheme(index)) => {
                return self.update_theme_picker(cx, |picker, cx| picker.highlight(index, cx));
            }
            C::Review(R::MoveTheme(delta)) => {
                return self.update_theme_picker(cx, |picker, cx| picker.move_highlight(delta, cx));
            }
            C::Review(R::CommitTheme) => {
                return self.update_theme_picker(cx, ThemePicker::commit);
            }
        }
        cx.notify();
        true
    }
}
