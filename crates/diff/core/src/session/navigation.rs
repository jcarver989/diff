use super::ReviewSession;
use crate::{DiffDocument, DiffReviewCommand, DiffSide, GapExpansion, GapId, SourceLocation, gaps_for_file};

const REVEAL_CONTEXT: usize = 3;

#[derive(Debug, Clone, Default)]
pub(super) struct Navigation {
    back: Vec<SourceLocation>,
    forward: Vec<SourceLocation>,
}

impl Navigation {
    pub(super) fn document_changed(&mut self, document: &DiffDocument, target_changed: bool) {
        if target_changed {
            self.back.clear();
            self.forward.clear();
            return;
        }
        let exists = |location: &SourceLocation| document.file_index(&location.path).is_some();
        self.back.retain(exists);
        self.forward.retain(exists);
    }
}

impl ReviewSession {
    pub fn apply_navigation(&mut self, command: DiffReviewCommand) -> Option<bool> {
        use DiffReviewCommand as C;
        Some(match command {
            C::JumpTo(location) => self.jump_to(&location),
            C::NavigateBack => self.navigate_back(),
            C::NavigateForward => self.navigate_forward(),
            _ => return None,
        })
    }

    pub fn jump_to(&mut self, location: &SourceLocation) -> bool {
        if self.draft.is_some() {
            return false;
        }
        let origin = self.selected_source_line();
        if !self.reveal_source(location) {
            return false;
        }
        if let Some(origin) = origin.filter(|origin| origin != location) {
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
    pub fn back_location(&self) -> Option<&SourceLocation> {
        self.navigation.back.last()
    }

    #[must_use]
    pub fn forward_location(&self) -> Option<&SourceLocation> {
        self.navigation.forward.last()
    }

    fn navigate(&mut self, back: bool) -> bool {
        loop {
            let next = if back { self.navigation.back.pop() } else { self.navigation.forward.pop() };
            let Some(location) = next else {
                return false;
            };
            let current = self.selected_source_line();
            if self.reveal_source(&location) {
                if let Some(current) = current {
                    if back { &mut self.navigation.forward } else { &mut self.navigation.back }.push(current);
                }
                return true;
            }
        }
    }

    fn reveal_source(&mut self, location: &SourceLocation) -> bool {
        let Some(file_index) = self.document.file_index(&location.path) else {
            return false;
        };
        if self.source_view().is_some_and(|path| path != &location.path) {
            self.projection.set_source_view(None);
            self.diff_return = None;
        }
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
        let file = self.document.files.get(file_index)?;
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
