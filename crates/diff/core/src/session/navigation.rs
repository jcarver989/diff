use super::ReviewSession;
use crate::{
    DiffDocument, DiffReviewCommand, DiffSide, FileDiff, GapExpansion, GapId, RepoPath, SourceDocument, SourceLocation,
    SourcePath, TargetLocation, gaps_for_file,
};
use std::sync::Arc;

const REVEAL_CONTEXT: usize = 3;

#[derive(Debug, Clone, Default)]
pub(super) struct Navigation {
    back: Vec<TargetLocation>,
    forward: Vec<TargetLocation>,
}

impl Navigation {
    pub(super) fn document_changed(&mut self, document: &DiffDocument, target_changed: bool) {
        if target_changed {
            self.back.clear();
            self.forward.clear();
            return;
        }
        let valid = |target: &TargetLocation| match &target.path {
            SourcePath::Repo(path) => target.side == DiffSide::New || document.file_index(path).is_some(),
            SourcePath::External { .. } => true,
        };
        self.back.retain(valid);
        self.forward.retain(valid);
    }
}

#[derive(Debug, Clone)]
pub(super) enum Reference {
    Pending { target: TargetLocation, error: Option<String> },
    Open { source: SourcePath, file: FileDiff },
}

impl Reference {
    pub(super) const fn file(&self) -> Option<&FileDiff> {
        match self {
            Self::Pending { .. } => None,
            Self::Open { file, .. } => Some(file),
        }
    }

    const fn source(&self) -> &SourcePath {
        match self {
            Self::Pending { target, .. } => &target.path,
            Self::Open { source, .. } => source,
        }
    }

    pub(super) fn survives(&self, document: &DiffDocument, target_changed: bool) -> bool {
        !target_changed && self.file().is_none_or(|file| document.file_index(&file.path).is_none())
    }

    pub(super) fn joins(&self, document: &DiffDocument, target_changed: bool) -> bool {
        !target_changed
            && matches!(self, Self::Open { source: SourcePath::Repo(path), .. } if document.file_index(path).is_some())
    }
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
        match &self.reference {
            Some(Reference::Pending { target, .. }) => Some(target),
            _ => None,
        }
    }

    #[must_use]
    pub fn reference_status(&self) -> Option<String> {
        let Some(Reference::Pending { target, error }) = &self.reference else {
            return None;
        };
        Some(match error {
            Some(error) => format!("Couldn't open {}: {error}", target.path),
            None => format!("Opening {}…", target.path),
        })
    }

    pub fn reference_failed(&mut self, source: &SourcePath, reason: impl Into<String>) {
        if let Some(Reference::Pending { target, error }) = &mut self.reference
            && &target.path == source
        {
            *error = Some(reason.into());
        }
    }

    #[must_use]
    pub fn reference(&self) -> Option<&FileDiff> {
        self.reference.as_ref().and_then(Reference::file)
    }

    #[must_use]
    pub fn reference_source(&self) -> Option<&SourcePath> {
        self.reference.as_ref().map(Reference::source)
    }

    #[must_use]
    pub const fn in_reference(&self) -> bool {
        self.reference.is_some()
    }

    #[must_use]
    pub fn shows_source(&self) -> bool {
        self.source_view().is_some() || self.in_reference()
    }

    #[must_use]
    pub fn shows_external(&self, path: &RepoPath) -> bool {
        matches!(&self.reference, Some(Reference::Open { source: SourcePath::External { .. }, file }) if &file.path == path)
    }

    pub(super) fn cancel_pending_reference(&mut self) {
        self.reference.take_if(|reference| matches!(reference, Reference::Pending { .. }));
    }

    pub fn open_reference(&mut self, source: &SourcePath, document: Arc<SourceDocument>) -> bool {
        let line_number = match &self.reference {
            Some(Reference::Pending { target, .. }) if &target.path == source => target.line_number,
            Some(Reference::Open { source: open, .. }) if open == source => {
                self.selected_source_line().map_or(1, |location| location.line_number)
            }
            _ => return false,
        };
        if let SourcePath::Repo(path) = source
            && self.document.file_index(path).is_some()
        {
            return self.reveal_source(&SourceLocation { path: path.clone(), side: DiffSide::New, line_number });
        }
        if self.source_view().is_none() {
            self.diff_return = self.capture_bookmark();
        }
        let path = source.display_path();
        let file = FileDiff::unchanged(path.clone(), document);
        self.reference = Some(Reference::Open { source: source.clone(), file });
        self.projection.set_source_view(Some(path.clone()));
        self.selected_file = self.document.files.len();
        self.pending_source_restore = Some(SourceLocation { path, side: DiffSide::New, line_number });
        self.rebuild();
        true
    }

    #[must_use]
    pub fn current_location(&self) -> Option<TargetLocation> {
        if let Some(pending) = self.pending_reference() {
            return Some(pending.clone());
        }
        let location = self.selected_source_line()?;
        Some(match &self.reference {
            Some(Reference::Open { source, file }) if file.path == location.path => {
                TargetLocation { path: source.clone(), side: location.side, line_number: location.line_number }
            }
            _ => location.into(),
        })
    }

    pub fn jump_to(&mut self, target: impl Into<TargetLocation>) -> bool {
        let target = target.into();
        if self.draft.is_some() {
            return false;
        }
        let origin = self.current_location();
        if !self.show(&target) {
            return false;
        }
        if let Some(origin) = origin.filter(|origin| origin != &target) {
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
        self.navigation.back.last()
    }

    #[must_use]
    pub fn forward_location(&self) -> Option<&TargetLocation> {
        self.navigation.forward.last()
    }

    fn navigate(&mut self, back: bool) -> bool {
        loop {
            let next = if back { self.navigation.back.pop() } else { self.navigation.forward.pop() };
            let Some(target) = next else {
                return false;
            };
            let current = self.current_location();
            if self.show(&target) {
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
            self.toggle_source_view();
        }
        self.reference = Some(Reference::Pending { target: target.clone(), error: None });
        true
    }

    fn shown_location(&self, target: &TargetLocation) -> Option<SourceLocation> {
        let path = match (&target.path, &self.reference) {
            (SourcePath::Repo(path), _) if self.file_index(path).is_some() => path.clone(),
            (source, Some(Reference::Open { source: open, file })) if open == source => file.path.clone(),
            _ => return None,
        };
        Some(SourceLocation { path, side: target.side, line_number: target.line_number })
    }

    fn reveal_source(&mut self, location: &SourceLocation) -> bool {
        self.cancel_pending_reference();
        if self.source_view().is_some_and(|path| path != &location.path) {
            self.projection.set_source_view(None);
            self.reference = None;
            self.diff_return = None;
            self.rebuild();
        }
        let Some(file_index) = self.file_index(&location.path) else {
            return false;
        };
        if self.presentation.row_showing_source(location).is_none() {
            if self.presentation.is_collapsed(file_index) {
                self.projection.set_collapsed(location.path.clone(), false);
            }
            if let Some((id, expansion)) = self.gap_expansion(file_index, location) {
                self.projection.set_expansion(id, expansion);
            }
            self.rebuild();
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
        let mut expansion = self.projection.expansions.get(&id).copied().unwrap_or_default();
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
