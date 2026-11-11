use super::{DiffViewerEvent, ReviewController, ReviewHandle, ViewerPane};
use clankerdiff_core::{
    DiffReviewCommand, DiffSide, FileTree, FileTreeEntry, FilesMode, LookupReply, RepoPath, SourceLocation, SourcePath,
    TargetLocation,
};
use clankerdiff_core::{InteractionPhase, ReviewCommand};
use clankerdiff_symbols::{Activation, DefinitionCard, Hold, Settled, SymbolIndex, SymbolReference};
use rouge_reactive::{sleep, spawn};
use std::{sync::Arc, time::Duration};

impl ReviewController {
    #[must_use]
    pub fn tree(&self) -> Option<&FileTree> {
        match self.session.files_mode() {
            FilesMode::Diff => Some(&self.sidebar),
            FilesMode::Browse => self.browse.as_ref(),
        }
    }

    fn tree_mut(&mut self) -> Option<&mut FileTree> {
        match self.session.files_mode() {
            FilesMode::Diff => Some(&mut self.sidebar),
            FilesMode::Browse => self.browse.as_mut(),
        }
    }

    pub(super) fn sidebar_changed(&mut self) {
        self.sidebar_revision = self.sidebar_revision.wrapping_add(1);
    }

    pub(crate) fn set_repository_files(&mut self, paths: &[RepoPath]) {
        self.browse.get_or_insert_default().set_repository(paths, self.session.document());
        self.sidebar_changed();
        self.sync_trees();
    }

    pub(crate) fn sync_trees(&mut self) {
        let shown = match self.session.pending_reference() {
            Some(TargetLocation { path: SourcePath::Repo(path), .. }) => Some(path),
            Some(_) => None,
            None => self.session.source_view(),
        };
        if let (Some(tree), Some(path)) = (&mut self.browse, shown) {
            let count = tree.entries().len();
            tree.reveal_path(path.as_str());
            if tree.entries().len() != count {
                self.sidebar_changed();
            }
        }
    }

    pub(crate) fn select_file(&mut self, index: usize) -> bool {
        if self.session.files_mode() == FilesMode::Browse {
            return self
                .session
                .document()
                .files
                .get(index)
                .map(|file| file.path.clone())
                .is_some_and(|path| self.browse_path(path));
        }
        if !self.session.select_file(index) {
            return false;
        }
        let count = self.sidebar.entries().len();
        self.sidebar.reveal_file(index);
        if self.sidebar.entries().len() != count {
            self.sidebar_changed();
        }
        true
    }

    fn browse_path(&mut self, path: RepoPath) -> bool {
        if self.session.source_view() == Some(&path) {
            return true;
        }
        self.navigate(DiffReviewCommand::JumpTo(TargetLocation {
            path: SourcePath::Repo(path),
            side: DiffSide::New,
            line_number: 1,
        }))
    }

    pub(crate) fn move_selection(&mut self, delta: isize) -> bool {
        if self.pane == ViewerPane::Diff {
            return self.session.move_row(delta);
        }
        let entry = self.tree_mut().and_then(|tree| tree.move_selection(delta).cloned());
        self.visit_entry(entry)
    }

    pub(crate) fn select_boundary(&mut self, end: bool) -> bool {
        if self.pane == ViewerPane::Diff {
            return self.session.select_boundary(end);
        }
        let entry = self.tree_mut().and_then(|tree| tree.select_boundary(end).cloned());
        self.visit_entry(entry)
    }

    fn visit_entry(&mut self, entry: Option<FileTreeEntry>) -> bool {
        let Some(entry) = entry else {
            return false;
        };
        if !entry.directory {
            if self.session.files_mode() == FilesMode::Browse {
                if let Ok(path) = RepoPath::new(&entry.path) {
                    self.browse_path(path);
                }
            } else if let Some(index) = entry.file {
                self.select_file(index);
            }
        }
        true
    }

    pub(crate) fn open_or_fold(&mut self, open: bool) -> bool {
        let Some(entry) = self.tree().and_then(FileTree::selected).cloned() else {
            return false;
        };
        if entry.directory {
            if entry.expanded != open
                && let Some(tree) = self.tree_mut()
            {
                tree.set_expanded(&entry.path, open);
                self.sidebar_changed();
            }
        } else if open {
            self.visit_entry(Some(entry));
            self.pane = ViewerPane::Diff;
        } else {
            return false;
        }
        true
    }

    pub(crate) fn toggle_source(&mut self) -> bool {
        if self.session.files_mode() == FilesMode::Diff
            && !self.session.shows_source()
            && self.pane == ViewerPane::Files
        {
            let Some(index) = self.sidebar.selected().and_then(|entry| entry.file) else {
                return false;
            };
            if self.session.selected_file() != Some(index) {
                self.select_file(index);
            }
        }
        let changed = if self.capabilities.lookups {
            self.session.set_files_mode(self.session.files_mode().toggled())
        } else {
            self.session.toggle_source_view()
        };
        if changed {
            self.pane = ViewerPane::Diff;
            self.sync_trees();
        }
        changed
    }

    pub(crate) fn navigate(&mut self, command: DiffReviewCommand) -> bool {
        if self.session.apply_navigation(command) != Some(true) {
            return false;
        }
        self.cancel_card_timer();
        if self.session.in_reference() && self.capabilities.lookups {
            self.session.set_files_mode(FilesMode::Browse);
        }
        self.names.navigated(&mut self.session, self.capabilities.lookups);
        self.names_changed();
        self.sync_trees();
        true
    }

    pub(crate) fn symbols(&mut self) -> Arc<SymbolIndex> {
        self.follow_syntax();
        self.syntax.symbols()
    }

    pub(crate) fn follow_syntax(&mut self) {
        let document = self.session.presentation().document();
        if !Arc::ptr_eq(self.syntax.document(), document) {
            self.syntax = Arc::new(self.syntax.followed(document));
        }
    }

    pub(crate) fn show_definition(&mut self) -> bool {
        let Some(origin) = self.session.selected_source_line() else {
            return false;
        };
        let index = self.symbols();
        if !self.names.open_at(origin, &self.session, &index, self.capabilities.lookups) {
            return false;
        }
        self.cancel_card_timer();
        self.names_changed();
        true
    }

    pub(crate) fn activate_definition(&mut self) -> bool {
        if self.names.card().is_none() {
            return false;
        }
        let index = self.symbols();
        match self.names.activate(&self.session, &index, self.capabilities.lookups) {
            Activation::Idle => false,
            Activation::Wait => {
                self.names_changed();
                true
            }
            Activation::Jump(target) => self.navigate(DiffReviewCommand::JumpTo(target)),
        }
    }

    pub(crate) fn cycle_definition(&mut self) -> bool {
        if self.names.card().is_none() {
            return false;
        }
        let index = self.symbols();
        if !self.names.cycle(&index) {
            return false;
        }
        self.cancel_card_timer();
        self.names_changed();
        true
    }

    pub(crate) fn close_definition(&mut self) -> bool {
        self.cancel_card_timer();
        let closed = self.names.close();
        if closed {
            self.names_changed();
        }
        closed
    }

    pub(crate) fn lookup_finished(&mut self, reply: LookupReply) {
        let index = self.symbols();
        match self.names.finish(reply, &mut self.session, &index) {
            Settled::Ignored => (),
            Settled::Redraw => self.names_changed(),
            Settled::Opened => {
                self.sync_trees();
                self.names_changed();
            }
            Settled::Jump(target) => {
                self.handle_command(DiffReviewCommand::JumpTo(target));
            }
        }
    }

    pub(super) fn cancel_card_timer(&mut self) {
        if let Some(task) = self.hide_card.take() {
            task.cancel();
        }
    }

    pub(crate) fn names_changed(&mut self) {
        self.definition_revision = self.definition_revision.wrapping_add(1);
        for lookup in self.names.take_lookups() {
            self.emit(DiffViewerEvent::Lookup(lookup));
        }
    }
}

impl ReviewHandle {
    pub fn hover_definition(&self, origin: SourceLocation, reference: &SymbolReference) {
        self.update(|state| {
            if !state.command_enabled(&DiffReviewCommand::ShowDefinition) {
                return;
            }
            state.cancel_card_timer();
            if state.names.card().is_some_and(|card| card.shows(&origin, &reference.span.columns)) {
                state.names.hover_token(true);
            } else {
                state.names.show(
                    DefinitionCard::new(origin, reference),
                    &state.session,
                    state.capabilities.lookups,
                    Hold::Hover,
                );
                state.names_changed();
            }
        });
    }

    pub fn set_definition_hovered(&self, token: bool, hovered: bool) {
        self.update(|state| {
            if token {
                state.names.hover_token(hovered);
            } else {
                state.names.hover_card(hovered);
            }
            state.cancel_card_timer();
            if state.names.lingering() {
                let handle = self.clone();
                let revision = state.definition_revision;
                state.hide_card = Some(spawn(async move {
                    sleep(Duration::from_millis(200)).await;
                    handle.update(|state| {
                        if state.definition_revision == revision && state.names.lingering() {
                            state.close_definition();
                        }
                    });
                }));
            }
        });
    }
}

impl ReviewHandle {
    pub(crate) fn line_command(&self, row: usize, side: DiffSide, command: DiffReviewCommand) -> bool {
        self.update(|state| {
            if state.command_context().phase != InteractionPhase::Browse {
                return false;
            }
            state.handle_command(DiffReviewCommand::Focus(ViewerPane::Diff));
            if !state.handle_command(DiffReviewCommand::SelectRow(row)) {
                return false;
            }
            state.handle_command(DiffReviewCommand::SelectSide(side));
            state.handle_command(command)
        })
        .unwrap_or(false)
    }

    pub(crate) fn select_cell(&self, row: usize, side: DiffSide) {
        self.line_command(row, side, DiffReviewCommand::SelectSide(side));
    }

    pub(crate) fn comment_action(&self, row: usize, side: DiffSide, id: u64, edit: bool) {
        self.update(|state| {
            if state.command_context().phase != InteractionPhase::Browse {
                return;
            }
            state.handle_command(DiffReviewCommand::Focus(ViewerPane::Diff));
            if !state.handle_command(DiffReviewCommand::SelectRow(row)) {
                return;
            }
            state.handle_command(DiffReviewCommand::SelectSide(side));
            let command = if edit { ReviewCommand::EditComment } else { ReviewCommand::DeleteComment };
            if !state.command_enabled(&command.into()) {
                return;
            }
            if edit {
                state.session.begin_draft(Some(id));
            } else {
                state.session.review_mut().remove_comment(id);
            }
        });
    }

    pub(crate) fn select_tree_entry(&self, index: usize, toggle: bool) {
        let before = self.selection().get_untracked().file;
        self.update(|state| {
            if !state.handle_command(DiffReviewCommand::Focus(ViewerPane::Files)) {
                return;
            }
            let entry = state.tree_mut().and_then(|tree| tree.select(index).cloned());
            if toggle && let Some(entry) = entry.as_ref().filter(|entry| entry.directory) {
                state.handle_command(if entry.expanded {
                    DiffReviewCommand::CollapseSelected
                } else {
                    DiffReviewCommand::OpenSelected
                });
            } else {
                state.visit_entry(entry);
            }
        });
        if !toggle && self.selection().get_untracked().file != before {
            self.reveal_selection(rouge_core::Align::Top);
        }
    }

    pub(crate) fn stage_tree_entry(&self, path: &str) {
        self.update(|state| {
            if let Some(action) = state.sidebar.toggle_stage_action(state.session.document(), path) {
                state.handle_command(DiffReviewCommand::RepositoryAction(action));
            }
        });
    }

    pub(crate) fn follow_visible_file(&self, file: usize) {
        self.update(|state| {
            state.focus_file = Some(file);
            if state.pane == ViewerPane::Diff && state.session.files_mode() == FilesMode::Diff {
                state.sidebar.follow_file(file);
            }
        });
    }
}
