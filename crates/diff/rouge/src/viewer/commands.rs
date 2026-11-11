use super::{Overlay, PickerCommand, ReviewController, ReviewOutput, ScrollRequest, ViewerPane};
use crate::DiffViewerEvent;
use clankerdiff_core::{
    CommandContext, DiffDocument, DiffReviewCommand as C, FilesMode, InteractionPhase, RepositoryAction,
    ReviewCapabilities, ReviewCommand as R,
};
use std::sync::Arc;

impl ReviewController {
    #[must_use]
    pub fn command_context(&self) -> CommandContext {
        CommandContext {
            phase: self.overlay.as_ref().map_or_else(
                || if self.session.draft().is_some() { InteractionPhase::Draft } else { InteractionPhase::Browse },
                Overlay::phase,
            ),
            capabilities: self.capabilities,
            repository_pending: self.repository_pending,
            themes_available: self.options.themes,
            commits_available: self.session.commits().is_some(),
            source_view: self.session.shows_source(),
            browsable: self.capabilities.lookups,
            ..CommandContext::default()
        }
    }

    #[must_use]
    pub fn command_enabled(&self, command: &C) -> bool {
        command.enabled(&self.command_context())
            && !matches!(command, C::Review(R::ScrollHelp(_)))
            && self.session.command_available(command)
    }

    pub(super) fn set_capabilities(&mut self, capabilities: ReviewCapabilities) {
        let lookups_changed = self.capabilities.lookups != capabilities.lookups;
        self.capabilities = capabilities;
        if !capabilities.repository
            && self.overlay.as_ref().is_some_and(|overlay| overlay.phase() == InteractionPhase::RepositoryPrompt)
        {
            self.overlay = None;
        }
        if lookups_changed {
            if capabilities.lookups {
                self.settle_files_mode();
            }
            self.cancel_card_timer();
            self.names.document_changed(&mut self.session, capabilities.lookups);
            self.names_changed();
        }
    }

    pub(super) fn set_document(&mut self, document: Arc<DiffDocument>) {
        if Arc::ptr_eq(self.session.document(), &document) {
            return;
        }
        let was_clean = self.session.document().files.is_empty();
        let target_changed = self.session.target() != &document.target;
        if target_changed {
            self.cancel_overlay();
        } else if matches!(self.overlay, Some(Overlay::Commits)) && self.session.commits() != document.commits.as_ref()
        {
            self.overlay = None;
        }
        self.sidebar.rebuild(&document);
        self.sidebar_changed();
        if let Some(tree) = &mut self.browse {
            tree.rebuild(&document);
        }
        self.session.set_document(document);
        if self.sidebar.selected().is_none()
            && let Some(file) = self.session.selected_file()
        {
            self.sidebar.reveal_file(file);
        }
        if was_clean != self.session.document().files.is_empty() {
            self.settle_files_mode();
        }
        self.cancel_card_timer();
        self.names.document_changed(&mut self.session, self.capabilities.lookups);
        self.names_changed();
        self.sync_trees();
    }

    fn settle_files_mode(&mut self) {
        if !self.capabilities.lookups {
            return;
        }
        let clean = self.session.document().files.is_empty();
        match self.session.files_mode() {
            FilesMode::Diff if clean && !self.session.in_reference() => {
                self.session.set_files_mode(FilesMode::Browse);
            }
            FilesMode::Browse if !clean && !self.session.shows_source() => {
                self.session.set_files_mode(FilesMode::Diff);
            }
            _ => (),
        }
        self.sync_trees();
    }

    #[expect(clippy::too_many_lines, reason = "Keep command routing exhaustive and centralized")]
    pub(super) fn handle_command(&mut self, command: C) -> bool {
        if !self.command_enabled(&command) {
            return false;
        }
        if !command.keeps_definition_card() {
            self.close_definition();
        }
        match command {
            C::Focus(pane) => self.pane = pane,
            C::ToggleFocus => {
                self.pane = if self.pane == ViewerPane::Diff { ViewerPane::Files } else { ViewerPane::Diff }
            }
            C::SelectFile(index) => return self.select_file(index),
            C::SelectRow(index) => return self.session.select_row(index),
            C::SelectSide(side) => {
                self.session.set_selected_side(side);
            }
            C::MoveSelection(delta) => return self.move_selection(delta),
            C::Page(delta) => {
                return self
                    .move_selection(delta.saturating_mul(self.page_rows[usize::from(self.pane == ViewerPane::Files)]));
            }
            C::First | C::Last => return self.select_boundary(command == C::Last),
            C::OpenSelected | C::CollapseSelected => return self.open_or_fold(command == C::OpenSelected),
            C::RevealGap(amount) => return self.session.reveal_selected_gap(amount),
            C::ToggleSourceView => return self.toggle_source(),
            C::SetFilesMode(mode) => {
                let changed = self.session.set_files_mode(mode);
                self.sync_trees();
                return changed;
            }
            C::ToggleFullFile => return self.session.toggle_full_file(),
            C::ToggleFileCollapsed(index) => {
                return index
                    .or_else(|| self.session.selected_file())
                    .is_some_and(|index| self.session.toggle_file_collapsed(index));
            }
            C::ToggleAllFilesCollapsed => return self.session.toggle_all_files_collapsed(),
            C::SetViewMode(_) | C::ToggleLayout => {
                let changed = if let C::SetViewMode(mode) = command {
                    self.session.set_view_mode(mode)
                } else {
                    self.session.toggle_layout()
                };
                if changed {
                    self.outputs.push(ReviewOutput::ViewModeChanged(self.session.view_mode()));
                }
            }
            C::ToggleStage => {
                let Some(action) = self
                    .sidebar
                    .selected()
                    .and_then(|entry| self.sidebar.toggle_stage_action(self.session.document(), &entry.path))
                else {
                    return false;
                };
                self.repository_action(action);
            }
            C::BeginCommit => self.overlay = Some(Overlay::Commit),
            C::BeginDiscard => {
                let Some(file) =
                    self.session.selected_file().and_then(|index| self.session.document().files.get(index))
                else {
                    return false;
                };
                self.overlay = Some(Overlay::Discard { path: file.path.clone(), status: file.status });
            }
            C::StageAll => self.repository_action(RepositoryAction::StageAll),
            C::UnstageAll => self.repository_action(RepositoryAction::UnstageAll),
            C::RepositoryAction(action) => self.repository_action(action),
            C::ToggleSidebar => {
                self.sidebar_collapsed = !self.sidebar_collapsed;
                if self.sidebar_collapsed {
                    self.pane = ViewerPane::Diff;
                }
                self.update_width();
            }
            C::ShowDefinition => return self.show_definition(),
            C::GoToDefinition => return self.activate_definition(),
            C::NextDefinition => return self.cycle_definition(),
            C::CloseDefinition => return self.close_definition(),
            C::JumpTo(_) | C::NavigateBack | C::NavigateForward => return self.navigate(command),
            C::SetScope(_) | C::CycleScope | C::StepTarget(_) | C::GoToPage(_) => {
                if matches!(self.overlay, Some(Overlay::Commits)) {
                    self.overlay = None;
                }
                if let Some(event) = self.session.target_event(&command) {
                    self.emit(event);
                } else {
                    return false;
                }
            }
            C::OpenCommitPicker => self.overlay = Some(Overlay::Commits),
            C::MoveCommitPicker(delta) | C::Review(R::MoveTheme(delta)) => {
                self.picker_command = Some(PickerCommand::Move(delta));
            }
            C::Refresh => self.emit(DiffViewerEvent::Refresh),
            C::SubmitReview => self.emit(DiffViewerEvent::SubmitReview(self.session.submission())),
            C::CopyReview => self.emit(DiffViewerEvent::CopyFormattedReview(self.session.submission().formatted)),
            C::Scroll { pane, lines } => self.scroll = Some(ScrollRequest { pane, lines }),
            C::Review(R::BeginComment) => return self.session.begin_draft(None),
            C::Review(R::EditComment) => return self.session.edit_comment_at_selection(),
            C::Review(R::DeleteComment) => return self.session.delete_comment_at_selection(),
            C::Review(R::UndoComment) => return self.session.undo_last_comment(),
            C::Review(R::SubmitComment) => {
                self.session.submit_draft();
            }
            C::Review(R::Cancel) => {
                if !self.cancel_overlay() {
                    if self.session.draft().is_some() {
                        self.session.cancel_draft();
                    } else {
                        self.emit(DiffViewerEvent::Cancel);
                    }
                }
            }
            C::Review(R::ShowHelp) => self.overlay = Some(Overlay::Help),
            C::Review(R::ScrollHelp(_)) => return false,
            C::Review(R::OpenThemePicker) => self.overlay = Some(Overlay::Themes(Box::new(self.theme.clone()))),
            C::Review(R::SelectTheme(index)) => self.picker_command = Some(PickerCommand::Highlight(index)),
            C::Review(R::CommitTheme) => self.picker_command = Some(PickerCommand::Choose),
        }
        true
    }

    fn repository_action(&mut self, action: RepositoryAction) {
        self.emit(DiffViewerEvent::RepositoryAction(action));
    }
}
