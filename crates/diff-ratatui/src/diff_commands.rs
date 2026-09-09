use crate::{
    DiffReviewCommand, DiffReviewEvent, DiffReviewState, DiffReviewStatus, FocusPane, InputOutcome,
    InteractionPhase, KeyBinding, KeyEvent, NavigationPane, ReviewCommand, keybindings,
    theme_picker::ThemePicker,
};
use clankerdiff_core::{CommandContext, RepositoryAction, ReviewCapabilities};
use std::sync::Arc;

impl DiffReviewState {
    #[must_use]
    pub fn command_for_key(&self, key: KeyEvent) -> Option<DiffReviewCommand> {
        if self.interaction_phase() != InteractionPhase::Browse {
            return None;
        }
        keybindings::binding_for_key(
            &self.keybindings,
            key,
            self.focus == FocusPane::Diff,
            self.layout().is_split(),
        )
        .map(|binding| binding.command.clone())
    }

    pub(crate) fn help_bindings(&self) -> impl Iterator<Item = &KeyBinding<DiffReviewCommand>> {
        let context = CommandContext {
            phase: InteractionPhase::Browse,
            ..self.command_context()
        };
        keybindings::help_bindings(
            &self.keybindings,
            context.navigation_available,
            move |command| self.command_enabled_in(command, &context),
        )
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
            navigation_available: !matches!(
                self.options.navigation,
                NavigationPane::Hidden | NavigationPane::Width(0)
            ),
            themes_available: !self.theme_choices.is_empty(),
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
        command.enabled(context)
            && (!matches!(command, DiffReviewCommand::CopyReview) || !self.review().is_empty())
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Exhaustive command dispatch keeps routing in one place"
    )]
    pub fn handle_command(
        &mut self,
        command: impl Into<DiffReviewCommand>,
    ) -> InputOutcome<DiffReviewEvent> {
        use DiffReviewCommand as C;
        use ReviewCommand as R;
        let command = command.into();
        if !self.command_enabled(&command) {
            return InputOutcome::Ignored;
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
                if !self
                    .session
                    .selected_file_range()
                    .is_some_and(|range| range.contains(&index))
                    || !self.session.select_row(index)
                {
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
            C::ToggleFullFile => {
                self.toggle_full_file();
                return InputOutcome::Consumed;
            }
            C::SetViewMode(mode) => {
                self.set_view_mode(mode);
                return InputOutcome::Consumed;
            }
            C::CycleViewMode => {
                if self.session.cycle_view_mode() {
                    self.scroll_to_selected_file();
                }
                return InputOutcome::Consumed;
            }
            C::SubmitReview => {
                return InputOutcome::Emitted(DiffReviewEvent::SubmitReview(
                    self.session.submission(),
                ));
            }
            C::CopyReview => {
                return InputOutcome::Emitted(DiffReviewEvent::CopyFormattedReview(
                    self.session.submission().formatted,
                ));
            }
            C::SetScope(scope) => return InputOutcome::Emitted(DiffReviewEvent::SetScope(scope)),
            C::CycleScope => {
                return InputOutcome::Emitted(DiffReviewEvent::SetScope(self.scope.next()));
            }
            C::Refresh => return InputOutcome::Emitted(DiffReviewEvent::Refresh),
            C::StageAll => {
                return InputOutcome::Emitted(DiffReviewEvent::RepositoryAction(
                    RepositoryAction::StageAll,
                ));
            }
            C::UnstageAll => {
                return InputOutcome::Emitted(DiffReviewEvent::RepositoryAction(
                    RepositoryAction::UnstageAll,
                ));
            }
            C::RepositoryAction(action) => {
                return InputOutcome::Emitted(DiffReviewEvent::RepositoryAction(action));
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
                self.help_scroll = self
                    .help_scroll
                    .saturating_add_signed(lines)
                    .min(self.help_bindings().count().saturating_sub(1));
            }
            C::Review(R::OpenThemePicker) => {
                self.theme_picker = ThemePicker::new(&self.theme, Arc::clone(&self.theme_choices));
            }
            C::Review(R::SelectTheme(index)) => {
                let Some(theme) = self
                    .theme_picker
                    .as_mut()
                    .and_then(|picker| picker.select(index))
                else {
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
