use super::{ActiveView, FilesMode, RestorePlan, ReviewSession, SourceTarget, SourceView};
use crate::{
    DiffDocument, DiffFileView, DiffReviewCommand, DiffSide, FileDiff, GapExpansion, GapId, RepoPath, SourceDisplay,
    SourceDocument, SourceFocus, SourceLocation, SourcePath, TargetLocation, gaps_for_file,
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
        match &self.active_view {
            ActiveView::Source(view) => match &view.target {
                SourceTarget::Reference { file, .. } => Some(file),
                SourceTarget::DiffFile(_) => None,
            },
            ActiveView::Diff => None,
        }
    }

    #[must_use]
    pub fn reference_source(&self) -> Option<&SourcePath> {
        if let Some(pending) = &self.pending_reference {
            return Some(&pending.target.path);
        }
        match &self.active_view {
            ActiveView::Source(view) => match &view.target {
                SourceTarget::Reference { source, .. } => Some(source),
                SourceTarget::DiffFile(_) => None,
            },
            ActiveView::Diff => None,
        }
    }

    #[must_use]
    pub fn in_reference(&self) -> bool {
        self.pending_reference.is_some() || self.reference().is_some()
    }

    #[must_use]
    pub const fn shows_source(&self) -> bool {
        matches!(self.active_view, ActiveView::Source(_)) || self.pending_reference.is_some()
    }

    #[must_use]
    pub fn shows_external(&self, path: &RepoPath) -> bool {
        match &self.active_view {
            ActiveView::Source(view) => matches!(
                &view.target,
                SourceTarget::Reference { source: SourcePath::External { .. }, file } if &file.path == path
            ),
            ActiveView::Diff => false,
        }
    }

    pub(super) fn cancel_pending_reference(&mut self) {
        self.pending_reference = None;
    }

    pub fn open_reference(&mut self, source: &SourcePath, document: Arc<SourceDocument>) -> bool {
        let line_number = if let Some(pending) = &self.pending_reference
            && &pending.target.path == source
        {
            pending.target.line_number
        } else if self.reference_source() == Some(source) && self.reference().is_some() {
            self.selected_source_line().map_or(1, |location| location.line_number)
        } else {
            return false;
        };
        if let SourcePath::Repo(path) = source
            && self.document.file_index(path).is_some()
        {
            return self.reveal_source(&SourceLocation { path: path.clone(), side: DiffSide::New, line_number });
        }
        self.pending_reference = None;
        let return_to = self.take_return_bookmark();
        let path = source.display_path();
        let file = FileDiff::unchanged(path.clone(), document);
        self.active_view = ActiveView::Source(Box::new(SourceView {
            target: SourceTarget::Reference { source: source.clone(), file },
            return_to,
        }));
        self.projection.set_source_focus(Some(SourceFocus::code(path.clone())));
        self.selected_file = self.document.files.len();
        self.rebuild(RestorePlan::JumpToSource(SourceLocation { path, side: DiffSide::New, line_number }));
        true
    }

    #[must_use]
    pub fn current_location(&self) -> Option<TargetLocation> {
        if let Some(pending) = self.pending_reference() {
            return Some(pending.clone());
        }
        let location = self.selected_source_line()?;
        Some(match &self.active_view {
            ActiveView::Source(view) => match &view.target {
                SourceTarget::Reference { source, file } if file.path == location.path => {
                    TargetLocation { path: source.clone(), side: location.side, line_number: location.line_number }
                }
                _ => location.into(),
            },
            ActiveView::Diff => location.into(),
        })
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
        if let Some(location) = self.shown_location(target) {
            return self.reveal_source(&location);
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

    fn shown_location(&self, target: &TargetLocation) -> Option<SourceLocation> {
        let path = match &target.path {
            SourcePath::Repo(path) if self.file_index(path).is_some() => path.clone(),
            source => match &self.active_view {
                ActiveView::Source(view) => match &view.target {
                    SourceTarget::Reference { source: open, file } if open == source => file.path.clone(),
                    _ => return None,
                },
                ActiveView::Diff => return None,
            },
        };
        Some(SourceLocation { path, side: target.side, line_number: target.line_number })
    }

    fn reveal_source(&mut self, location: &SourceLocation) -> bool {
        self.cancel_pending_reference();
        let elsewhere = self.source_view().is_none_or(|path| path != &location.path);
        if self.files_mode == FilesMode::Browse && elsewhere {
            self.browse_to(location);
        } else if matches!(self.active_view, ActiveView::Source(_)) && elsewhere {
            self.projection.set_source_focus(None);
            self.active_view = ActiveView::Diff;
            self.rebuild(RestorePlan::PreserveSelection);
        } else if self.projection.source_focus().is_some_and(|focus| focus.display == SourceDisplay::MarkdownPreview) {
            self.projection.set_source_display(SourceDisplay::Code);
            self.rebuild(RestorePlan::JumpToSource(location.clone()));
        }
        let Some(file_index) = self.file_index(&location.path) else {
            return false;
        };
        if self.presentation.row_showing_source(location).is_none() {
            self.projection.set_file_view(location.path.clone(), DiffFileView::Patch);
            if let Some((id, expansion)) = self.gap_expansion(file_index, location) {
                self.projection.set_expansion(id, expansion);
            }
            self.rebuild(RestorePlan::PreserveSelection);
        }
        let Some(row) = self.presentation.row_showing_source(location) else {
            return false;
        };
        if !self.select_row(row) {
            return false;
        }
        self.set_selected_side(location.side);
        true
    }

    fn browse_to(&mut self, location: &SourceLocation) {
        let return_to = self.take_return_bookmark();
        self.active_view = ActiveView::Source(Box::new(SourceView {
            target: SourceTarget::DiffFile(location.path.clone()),
            return_to,
        }));
        self.projection.set_source_focus(Some(SourceFocus::code(location.path.clone())));
        self.rebuild(RestorePlan::JumpToSource(location.clone()));
    }

    fn gap_expansion(&self, file_index: usize, location: &SourceLocation) -> Option<(GapId, GapExpansion)> {
        let file = self.file(file_index)?;
        let count = |side| file.source_document(side).map_or(0, |source| source.line_count());
        let gaps = gaps_for_file(file, count(DiffSide::Old), count(DiffSide::New));
        let line = location.line_number;
        let (gap_index, range) = gaps.iter().enumerate().find_map(|(index, gap)| {
            let range = match location.side {
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
