use crate::{
    DiffReviewCommand, DiffReviewEvent, DiffReviewState, DiffReviewStatus, FocusPane, InputOutcome, InteractionPhase,
    KeyBinding, KeyEvent, NavigationPane, ReviewCommand, keybindings, theme_picker::ThemePicker,
};
use clankerdiff_core::{CommandContext, CommitPicker, RepositoryAction, ReviewCapabilities};
use std::sync::Arc;
use web_time::{Duration, SystemTime, UNIX_EPOCH};

impl DiffReviewState {
    #[must_use]
    pub fn command_for_key(&self, key: KeyEvent) -> Option<DiffReviewCommand> {
        if self.interaction_phase() != InteractionPhase::Browse {
            return None;
        }
        keybindings::binding_for_key(&self.keybindings, key, self.focus == FocusPane::Diff, self.layout().is_split())
            .map(|binding| binding.command.clone())
    }

    pub(crate) fn help_bindings(&self) -> impl Iterator<Item = &KeyBinding<DiffReviewCommand>> {
        let context = CommandContext { phase: InteractionPhase::Browse, ..self.command_context() };
        keybindings::help_bindings(&self.keybindings, context.navigation_available, move |command| {
            self.command_enabled_in(command, &context)
        })
    }

    pub(crate) fn footer_hint(&self, width: usize) -> String {
        keybindings::footer_hint(
            &self.keybindings,
            self.focus == FocusPane::Diff,
            self.layout().is_split(),
            |command| self.command_enabled(command),
            &ReviewCommand::ShowHelp.into(),
            width,
        )
    }

    #[must_use]
    pub fn command_context(&self) -> CommandContext {
        CommandContext {
            phase: self.interaction_phase(),
            capabilities: self.capabilities,
            repository_pending: self.repository_pending(),
            document_ready: matches!(self.status, DiffReviewStatus::Ready),
            source_view: self.session.shows_source(),
            navigation_available: !matches!(self.options.navigation, NavigationPane::Hidden | NavigationPane::Width(0)),
            themes_available: !self.theme_choices.is_empty(),
            commits_available: self.session.commits().is_some(),
        }
    }

    pub fn set_capabilities(&mut self, capabilities: ReviewCapabilities) {
        self.capabilities = capabilities;
        if !capabilities.repository {
            self.repository_prompt = None;
        }
        self.mark_dirty();
    }

    #[must_use]
    pub fn command_enabled(&self, command: &DiffReviewCommand) -> bool {
        self.command_enabled_in(command, &self.command_context())
    }

    fn command_enabled_in(&self, command: &DiffReviewCommand, context: &CommandContext) -> bool {
        command.enabled(context) && (!matches!(command, DiffReviewCommand::CopyReview) || !self.review().is_empty())
    }

    pub fn handle_command(&mut self, command: impl Into<DiffReviewCommand>) -> InputOutcome<DiffReviewEvent> {
        let outcome = self.dispatch_command(command.into());
        self.install_deferred();
        outcome
    }

    #[expect(clippy::too_many_lines, reason = "Exhaustive command dispatch keeps routing in one place")]
    fn dispatch_command(&mut self, command: DiffReviewCommand) -> InputOutcome<DiffReviewEvent> {
        use DiffReviewCommand as C;
        use ReviewCommand as R;
        if !self.command_enabled(&command) {
            return InputOutcome::Ignored;
        }
        if self.names.close() {
            self.mark_dirty();
        }
        match command {
            C::Focus(pane) => self.focus = pane,
            C::ToggleFocus => {
                self.focus = match self.focus {
                    FocusPane::Files => FocusPane::Diff,
                    FocusPane::Diff => FocusPane::Files,
                }
            }
            C::SelectFile(index) => {
                if !self.select_file(index) {
                    return InputOutcome::Ignored;
                }
            }
            C::MoveSelection(delta) => match self.focus {
                FocusPane::Files => self.move_drawer_entry(delta),
                FocusPane::Diff => self.move_row(delta),
            },
            C::Scroll { pane, lines } => {
                match pane {
                    FocusPane::Files => self.scroll_drawer(lines),
                    FocusPane::Diff => self.scroll_patch(lines),
                }
                return InputOutcome::Consumed;
            }
            C::Page(delta) => self.page(delta),
            C::First => self.select_boundary(false),
            C::Last => self.select_boundary(true),
            C::OpenSelected => {
                if !self.expand_or_open_drawer_entry() {
                    self.focus = FocusPane::Diff;
                }
            }
            C::CollapseSelected => self.collapse_drawer_entry(),
            C::ToggleStage => {
                let Some(action) = self.toggle_stage_action() else {
                    return InputOutcome::Ignored;
                };
                return InputOutcome::Emitted(DiffReviewEvent::RepositoryAction(action));
            }
            C::BeginCommit => self.begin_commit(),
            C::BeginDiscard => self.begin_discard(),
            C::SelectRow(index) => {
                let previous = (self.session.selected_row(), self.session.selected_side());
                if !self.session.select_row(index) {
                    return InputOutcome::Ignored;
                }
                if previous == (self.session.selected_row(), self.session.selected_side()) {
                    return InputOutcome::Consumed;
                }
                self.request_follow();
            }
            C::SelectSide(side) => {
                let previous = self.session.selected_side();
                self.session.set_selected_side(side);
                if previous == self.session.selected_side() {
                    return InputOutcome::Consumed;
                }
                self.request_follow();
            }
            C::RevealGap(amount) => {
                self.reveal_selected_gap(amount);
                return InputOutcome::Consumed;
            }
            C::ToggleSourceView => {
                if self.session.source_view().is_none() && self.focus == FocusPane::Files {
                    let Some(index) = self.drawer.selected().and_then(|entry| entry.file) else {
                        return InputOutcome::Ignored;
                    };
                    if self.session.selected_file() != Some(index) {
                        self.select_file(index);
                    }
                }
                if !self.toggle_source_view() {
                    return InputOutcome::Ignored;
                }
                self.focus = FocusPane::Diff;
                return InputOutcome::Consumed;
            }
            C::ToggleFullFile => {
                self.toggle_full_file();
                return InputOutcome::Consumed;
            }
            C::ToggleFileCollapsed(index) => {
                let Some(index) = index.or_else(|| self.session.selected_file()) else {
                    return InputOutcome::Ignored;
                };
                if !self.toggle_file_collapsed(index) {
                    return InputOutcome::Ignored;
                }
                return InputOutcome::Consumed;
            }
            C::ToggleAllFilesCollapsed => {
                if !self.toggle_all_files_collapsed() {
                    return InputOutcome::Ignored;
                }
                return InputOutcome::Consumed;
            }
            C::SetViewMode(mode) => {
                if !self.change_layout(|state| state.session.set_view_mode(mode)) {
                    return InputOutcome::Consumed;
                }
                return InputOutcome::ViewModeSelected(mode);
            }
            C::ToggleLayout => {
                self.change_layout(|state| state.session.toggle_layout());
                return InputOutcome::ViewModeSelected(self.view_mode());
            }
            C::SubmitReview => {
                return InputOutcome::Emitted(DiffReviewEvent::SubmitReview(self.session.submission()));
            }
            C::CopyReview => {
                return InputOutcome::Emitted(DiffReviewEvent::CopyFormattedReview(
                    self.session.submission().formatted,
                ));
            }
            C::SetScope(_) | C::CycleScope | C::StepTarget(_) | C::GoToPage(_) => {
                self.commit_picker = None;
                self.mark_dirty();
                return self.session.target_event(&command).map_or(InputOutcome::Ignored, InputOutcome::Emitted);
            }
            C::OpenCommitPicker => {
                self.commit_picker = self.session.commit_pages(since_epoch()).and_then(CommitPicker::new);
            }
            C::MoveCommitPicker(delta) => {
                if let Some(picker) = self.commit_picker.as_mut() {
                    picker.move_by(delta);
                }
            }
            C::Refresh => return InputOutcome::Emitted(DiffReviewEvent::Refresh),
            C::StageAll => {
                return InputOutcome::Emitted(DiffReviewEvent::RepositoryAction(RepositoryAction::StageAll));
            }
            C::UnstageAll => {
                return InputOutcome::Emitted(DiffReviewEvent::RepositoryAction(RepositoryAction::UnstageAll));
            }
            C::RepositoryAction(action) => {
                return InputOutcome::Emitted(DiffReviewEvent::RepositoryAction(action));
            }
            C::ShowDefinition => {
                if !self.show_definition_at_selection() {
                    return InputOutcome::Ignored;
                }
            }
            C::JumpTo(_) | C::NavigateBack | C::NavigateForward => {
                if self.session.apply_navigation(command) != Some(true) {
                    return InputOutcome::Ignored;
                }
                self.request_follow();
                self.names.navigated(&mut self.session, self.capabilities.lookups);
            }
            C::Review(R::BeginComment | R::EditComment) => {
                let started = if command == C::Review(R::BeginComment) {
                    self.session.begin_draft(None)
                } else {
                    self.session.edit_comment_at_selection()
                };
                if !started {
                    return InputOutcome::Ignored;
                }
                self.request_follow();
            }
            C::Review(R::DeleteComment | R::UndoComment) => {
                let changed = if command == C::Review(R::DeleteComment) {
                    self.session.delete_comment_at_selection()
                } else {
                    self.session.undo_last_comment()
                };
                if !changed {
                    return InputOutcome::Ignored;
                }
                self.request_follow();
            }
            C::Review(R::SubmitComment) => {
                self.session.submit_draft();
                self.cursor_position = None;
                self.request_follow();
            }
            C::Review(R::Cancel) => match self.interaction_phase() {
                InteractionPhase::Browse => return InputOutcome::Emitted(DiffReviewEvent::Cancel),
                InteractionPhase::Draft => {
                    self.session.cancel_draft();
                    self.cursor_position = None;
                    self.request_follow();
                }
                InteractionPhase::Help => self.help = false,
                InteractionPhase::CommitPicker => self.commit_picker = None,
                InteractionPhase::RepositoryPrompt => self.repository_prompt = None,
                InteractionPhase::ThemePicker => {
                    if let Some(picker) = self.theme_picker.take() {
                        self.apply_theme(picker.cancel());
                    }
                }
            },
            C::Review(R::ShowHelp) => {
                self.help = true;
                self.help_scroll = 0;
            }
            C::Review(R::ScrollHelp(lines)) => {
                self.help_scroll =
                    self.help_scroll.saturating_add_signed(lines).min(self.help_bindings().count().saturating_sub(1));
            }
            C::Review(R::OpenThemePicker) => {
                self.theme_picker = ThemePicker::new(&self.theme, Arc::clone(&self.theme_choices));
            }
            C::Review(R::SelectTheme(index)) => {
                let Some(theme) = self.theme_picker.as_mut().and_then(|picker| picker.select(index)) else {
                    return InputOutcome::Ignored;
                };
                self.apply_theme(theme);
            }
            C::Review(R::MoveTheme(delta)) => {
                if let Some(picker) = self.theme_picker.as_mut() {
                    let theme = picker.select_relative(delta);
                    self.apply_theme(theme);
                }
            }
            C::Review(R::CommitTheme) => {
                let Some(picker) = self.theme_picker.take() else {
                    return InputOutcome::Ignored;
                };
                let theme = picker.commit();
                let id = theme.id().clone();
                self.apply_theme(theme);
                return InputOutcome::ThemeSelected(id);
            }
        }
        self.mark_dirty();
        InputOutcome::Consumed
    }
}

fn since_epoch() -> Duration {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default()
}
