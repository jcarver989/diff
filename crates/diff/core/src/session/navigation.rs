use super::{FilesMode, ReviewSession, document_with_reference};
use crate::{
    DiffDocument, DiffFileView, DiffReviewCommand, DiffSide, FileDiff, GapExpansion, GapId, RepoPath, SourceDocument,
    SourcePath, TargetLocation, gaps_for_file,
};
use std::sync::Arc;

const REVEAL_CONTEXT: usize = 3;

#[derive(Debug, Clone, Default)]
pub(super) struct Navigation {
    back: Vec<Visit>,
    forward: Vec<Visit>,
}

#[derive(Debug, Clone)]
struct Visit {
    target: TargetLocation,
    mode: FilesMode,
}

impl Navigation {
    pub(super) fn document_changed(&mut self, document: &DiffDocument, target_changed: bool) {
        if target_changed {
            self.back.clear();
            self.forward.clear();
            return;
        }
        let valid = |Visit { target, .. }: &Visit| match &target.path {
            SourcePath::Repo(path) => target.side == DiffSide::New || document.file_index(path).is_some(),
            SourcePath::External { .. } => true,
        };
        self.back.retain(valid);
        self.forward.retain(valid);
    }
}

#[derive(Debug, Clone)]
pub(super) struct PendingReference {
    pub(super) target: TargetLocation,
    pub(super) error: Option<String>,
}

impl ReviewSession {
    pub fn apply_navigation(&mut self, command: DiffReviewCommand) -> Option<bool> {
        use DiffReviewCommand as C;
        Some(match command {
            C::JumpTo(target) => self.jump_to(target),
            C::NavigateBack => self.navigate_back(),
            C::NavigateForward => self.navigate_forward(),
            _ => return None,
        })
    }

    #[must_use]
    pub const fn pending_reference(&self) -> Option<&TargetLocation> {
        match &self.pending_reference {
            Some(pending) => Some(&pending.target),
            None => None,
        }
    }

    #[must_use]
    pub fn reference_status(&self) -> Option<String> {
        let pending = self.pending_reference.as_ref()?;
        Some(match &pending.error {
            Some(error) => format!("Couldn't open {}: {error}", pending.target.path),
            None => format!("Opening {}…", pending.target.path),
        })
    }

    pub fn reference_failed(&mut self, source: &SourcePath, reason: impl Into<String>) {
        if let Some(pending) = &mut self.pending_reference
            && &pending.target.path == source
        {
            pending.error = Some(reason.into());
        }
    }

    #[must_use]
    pub fn reference(&self) -> Option<&FileDiff> {
        self.source.as_ref()?.reference.as_ref()?.files.last()
    }

    #[must_use]
    pub fn reference_source(&self) -> Option<&SourcePath> {
        self.pending_reference()
            .map(|t| &t.path)
            .or_else(|| self.source.as_ref().filter(|s| s.reference.is_some()).map(|s| &s.target.path))
    }

    #[must_use]
    pub fn in_reference(&self) -> bool {
        self.pending_reference.is_some() || self.reference().is_some()
    }

    #[must_use]
    pub const fn shows_source(&self) -> bool {
        self.source.is_some() || self.pending_reference.is_some()
    }

    #[must_use]
    pub fn shows_external(&self, path: &RepoPath) -> bool {
        self.source.as_ref().is_some_and(|s| matches!(s.target.path, SourcePath::External { .. }))
            && self.source_view() == Some(path)
    }

    pub(super) fn cancel_pending_reference(&mut self) {
        self.pending_reference = None;
    }

    pub fn open_reference(&mut self, source: &SourcePath, document: Arc<SourceDocument>) -> bool {
        if self.reference_source() != Some(source) {
            return false;
        }
        let Some(target) = self.current_location() else {
            return false;
        };
        let preview = self.pending_reference.is_none() && self.source.as_ref().is_some_and(|s| s.preview);
        self.pending_reference = None;
        if let SourcePath::Repo(path) = source
            && let Some(index) = self.document.file_index(path)
        {
            return self.reveal_source(index, &target);
        }
        let file = FileDiff::unchanged(source.display_path(), document);
        self.focus_source(target, Some(document_with_reference(&self.document, file)), preview);
        true
    }

    #[must_use]
    pub fn current_location(&self) -> Option<TargetLocation> {
        self.pending_reference()
            .or_else(|| self.source.as_ref().map(|s| &s.target))
            .cloned()
            .or_else(|| self.selected_source_line().map(Into::into))
    }

    pub fn jump_to(&mut self, target: impl Into<TargetLocation>) -> bool {
        let target = target.into();
        if self.draft.is_some() {
            return false;
        }
        let origin = self.current_visit();
        if !self.show(&target) {
            return false;
        }
        if let Some(origin) = origin.filter(|origin| origin.target != target) {
            self.navigation.back.push(origin);
        }
        self.navigation.forward.clear();
        true
    }

    pub fn navigate_back(&mut self) -> bool {
        self.navigate(true)
    }

    pub fn navigate_forward(&mut self) -> bool {
        self.navigate(false)
    }

    #[must_use]
    pub fn back_location(&self) -> Option<&TargetLocation> {
        self.navigation.back.last().map(|visit| &visit.target)
    }

    #[must_use]
    pub fn forward_location(&self) -> Option<&TargetLocation> {
        self.navigation.forward.last().map(|visit| &visit.target)
    }

    fn current_visit(&self) -> Option<Visit> {
        Some(Visit { target: self.current_location()?, mode: self.files_mode })
    }

    fn navigate(&mut self, back: bool) -> bool {
        loop {
            let next = if back { self.navigation.back.pop() } else { self.navigation.forward.pop() };
            let Some(visit) = next else {
                return false;
            };
            let current = self.current_visit();
            self.set_files_mode(visit.mode);
            if self.show(&visit.target) {
                if let Some(current) = current {
                    if back { &mut self.navigation.forward } else { &mut self.navigation.back }.push(current);
                }
                return true;
            }
        }
    }

    fn show(&mut self, target: &TargetLocation) -> bool {
        let index = match &target.path {
            SourcePath::Repo(path) if let Some(index) = self.document.file_index(path) => Some(index),
            path if self.source.as_ref().is_some_and(|s| s.reference.is_some() && s.target.path == *path) => {
                Some(self.selected_file)
            }
            _ => None,
        };
        if let Some(index) = index {
            return self.reveal_source(index, target);
        }
        if target.side == DiffSide::Old || target.line_number == 0 {
            return false;
        }
        if self.reference().is_some() {
            self.leave_source_view();
        }
        self.pending_reference = Some(PendingReference { target: target.clone(), error: None });
        true
    }

    fn reveal_source(&mut self, file_index: usize, target: &TargetLocation) -> bool {
        self.cancel_pending_reference();
        let elsewhere = self.source.as_ref().is_none_or(|s| s.target.path != target.path);
        if self.files_mode == FilesMode::Browse && elsewhere {
            self.focus_source(target.clone(), None, false);
        } else if self.source.is_some() && elsewhere {
            self.leave_source_view();
        } else if let Some(view) = &mut self.source
            && view.preview
        {
            view.preview = false;
            view.target.line_number = target.line_number;
            self.rebuild();
        }
        if self.source.is_none() && self.presentation.source_row(file_index, target.side, target.line_number).is_none()
        {
            self.projection.set_file_view(target.path.display_path(), DiffFileView::Patch);
            if let Some((id, expansion)) = self.gap_expansion(file_index, target) {
                self.projection.set_expansion(id, expansion);
            }
            self.rebuild();
        }
        let Some(row) = self.presentation.source_row(file_index, target.side, target.line_number) else {
            return false;
        };
        if !self.select_row(row) {
            return false;
        }
        self.set_selected_side(target.side);
        true
    }

    fn gap_expansion(&self, file_index: usize, target: &TargetLocation) -> Option<(GapId, GapExpansion)> {
        let file = self.file(file_index)?;
        let count = |side| file.source_document(side).map_or(0, |source| source.line_count());
        let gaps = gaps_for_file(file, count(DiffSide::Old), count(DiffSide::New));
        let line = target.line_number;
        let (gap_index, range) = gaps.iter().enumerate().find_map(|(index, gap)| {
            let range = match target.side {
                DiffSide::Old => &gap.old,
                DiffSide::New => &gap.new,
            };
            range.contains(&line).then(|| (index, range.clone()))
        })?;
        let id = GapId { file_index, gap_index };
        let mut expansion = self.projection.expansion(id);
        let leading = line - range.start + 1 + REVEAL_CONTEXT;
        let trailing = range.end - line + REVEAL_CONTEXT;
        let unbounded = file.hunks.is_empty();
        let can_lead = gap_index != 0 || unbounded;
        let can_trail = gap_index != file.hunks.len() || unbounded;
        if can_lead && (!can_trail || leading <= trailing) {
            expansion.revealed_prefix = expansion.revealed_prefix.max(leading);
        } else {
            expansion.revealed_suffix = expansion.revealed_suffix.max(trailing);
        }
        Some((id, expansion))
    }
}
