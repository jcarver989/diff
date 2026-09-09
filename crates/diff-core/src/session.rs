use crate::{
    ContentProjection, DiffDocument, DiffPresentation, DiffSide, GapId, Layout, LineAnchor,
    PresentationOptions, PresentedCell, PresentedRow, Review, ReviewSubmission, SourceLocation,
    ViewMode, presentation::retained_expansions,
};
use std::{ops::Range, sync::Arc};

const EXPAND_STEP: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevealAmount {
    Step,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionOptions {
    pub include_file_headers: bool,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            include_file_headers: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentDraft {
    anchor: LineAnchor,
    line_text: String,
    body: String,
    cursor: usize,
    editing: Option<u64>,
}

impl CommentDraft {
    fn new(anchor: LineAnchor, line_text: String, body: String, editing: Option<u64>) -> Self {
        Self {
            anchor,
            line_text,
            cursor: body.len(),
            body,
            editing,
        }
    }

    #[must_use]
    pub const fn anchor(&self) -> &LineAnchor {
        &self.anchor
    }

    #[must_use]
    pub fn line_text(&self) -> &str {
        &self.line_text
    }

    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    #[must_use]
    pub fn body_before_cursor(&self) -> &str {
        &self.body[..self.cursor]
    }

    #[must_use]
    pub const fn editing(&self) -> Option<u64> {
        self.editing
    }

    pub fn set_body(&mut self, body: impl Into<String>) {
        self.body = body.into();
        self.cursor = self.body.len();
    }

    pub fn insert(&mut self, text: &str) {
        self.body.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    pub fn delete_before_cursor(&mut self) {
        let previous = self.previous_boundary();
        self.body.replace_range(previous..self.cursor, "");
        self.cursor = previous;
    }

    pub fn delete_at_cursor(&mut self) {
        let next = self.next_boundary();
        self.body.replace_range(self.cursor..next, "");
    }

    pub fn move_cursor_left(&mut self) {
        self.cursor = self.previous_boundary();
    }

    pub fn move_cursor_right(&mut self) {
        self.cursor = self.next_boundary();
    }

    pub const fn move_cursor_to_start(&mut self) {
        self.cursor = 0;
    }

    pub fn move_cursor_to_end(&mut self) {
        self.cursor = self.body.len();
    }

    fn previous_boundary(&self) -> usize {
        self.body[..self.cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(offset, _)| offset)
    }

    fn next_boundary(&self) -> usize {
        self.body[self.cursor..]
            .char_indices()
            .nth(1)
            .map_or(self.body.len(), |(offset, _)| self.cursor + offset)
    }
}

#[derive(Debug, Clone)]
pub struct ReviewSession {
    document: Arc<DiffDocument>,
    presentation: DiffPresentation,
    review: Review,
    options: SessionOptions,
    view_mode: ViewMode,
    split_when_auto: bool,
    selected_file: usize,
    selected_row: usize,
    selected_side: DiffSide,
    draft: Option<CommentDraft>,
    projection: ContentProjection,
    projection_revision: u64,
    pending_source_restore: Option<SourceLocation>,
}

impl ReviewSession {
    #[must_use]
    pub fn new(document: Arc<DiffDocument>) -> Self {
        Self::with_options(document, SessionOptions::default())
    }

    #[must_use]
    pub fn with_options(document: Arc<DiffDocument>, options: SessionOptions) -> Self {
        let mut session = Self {
            presentation: DiffPresentation::new(
                document.clone(),
                PresentationOptions {
                    view_mode: ViewMode::Auto,
                    split_when_auto: false,
                    include_file_headers: options.include_file_headers,
                },
            ),
            document,
            review: Review::default(),
            options,
            view_mode: ViewMode::Auto,
            split_when_auto: false,
            selected_file: 0,
            selected_row: 0,
            selected_side: DiffSide::New,
            draft: None,
            projection: ContentProjection::default(),
            projection_revision: 0,
            pending_source_restore: None,
        };
        session.select_first_row();
        session
    }

    #[must_use]
    pub const fn document(&self) -> &Arc<DiffDocument> {
        &self.document
    }

    #[must_use]
    pub const fn presentation(&self) -> &DiffPresentation {
        &self.presentation
    }

    #[must_use]
    pub const fn review(&self) -> &Review {
        &self.review
    }

    pub const fn review_mut(&mut self) -> &mut Review {
        &mut self.review
    }

    #[must_use]
    pub const fn view_mode(&self) -> ViewMode {
        self.view_mode
    }

    #[must_use]
    pub const fn layout(&self) -> Layout {
        self.view_mode.resolve(self.split_when_auto)
    }

    #[must_use]
    pub fn selected_file(&self) -> Option<usize> {
        (!self.document.files.is_empty()).then_some(self.selected_file)
    }

    #[must_use]
    pub fn selected_row(&self) -> Option<usize> {
        (self.presentation.row_count() != 0).then_some(self.selected_row)
    }

    #[must_use]
    pub const fn selected_side(&self) -> DiffSide {
        self.selected_side
    }

    #[must_use]
    pub fn selected_file_range(&self) -> Option<Range<usize>> {
        self.presentation.file_range(self.selected_file)
    }

    #[must_use]
    pub const fn draft(&self) -> Option<&CommentDraft> {
        self.draft.as_ref()
    }

    pub const fn draft_mut(&mut self) -> Option<&mut CommentDraft> {
        self.draft.as_mut()
    }

    #[must_use]
    pub const fn projection_revision(&self) -> u64 {
        self.projection_revision
    }

    #[must_use]
    pub fn selected_source_line(&self) -> Option<SourceLocation> {
        let row = self.selected_presented_row()?;
        self.presentation
            .source_location(row, row.preferred_cell(self.selected_side)?)
    }

    pub fn reveal_selected_gap(&mut self, amount: RevealAmount) -> bool {
        let id = self
            .presentation
            .gap_info(self.selected_row)
            .map(|info| info.id)
            .or_else(|| {
                let row = self.presentation.row(self.selected_row)?;
                let source = row.preferred_cell(self.selected_side)?.patch_source?;
                let hunk = self
                    .document
                    .files
                    .get(row.file_index)?
                    .hunks
                    .get(source.hunk_index)?;
                // Code rows reveal the nearest hunk edge when no gap row is selected.
                let after = source.line_index.saturating_mul(2) >= hunk.lines.len();
                Some(GapId {
                    file_index: row.file_index,
                    gap_index: source.hunk_index + usize::from(after),
                })
            });
        let Some(id) = id else {
            return false;
        };
        let Some(file) = self.document.files.get(id.file_index) else {
            return false;
        };
        let hunk_count = file.hunks.len();
        let expansion = self.projection.expansions.entry(id).or_default();
        let amount = match amount {
            RevealAmount::Step => EXPAND_STEP,
            RevealAmount::All => usize::MAX,
        };
        if id.gap_index != 0 || hunk_count == 0 {
            expansion.revealed_prefix = expansion.revealed_prefix.saturating_add(amount);
        }
        if id.gap_index != hunk_count || hunk_count == 0 {
            expansion.revealed_suffix = expansion.revealed_suffix.saturating_add(amount);
        }
        self.rebuild();
        true
    }

    pub fn toggle_full_file(&mut self) -> bool {
        let Some(file) = self.document.files.get(self.selected_file) else {
            return false;
        };
        if file.binary || file.omitted_bytes.is_some() {
            return false;
        }
        let path = file.path.clone();
        if !self.projection.full_files.remove(&path) {
            self.projection.full_files.insert(path);
        }
        self.rebuild();
        true
    }

    /// Replaces the complete immutable document while preserving review and view
    /// state, including gap expansions for files whose content did not change.
    pub fn set_document(&mut self, document: Arc<DiffDocument>) {
        let expansions =
            retained_expansions(&self.document, &self.projection.expansions, &document);
        let selected_path = self
            .document
            .files
            .get(self.selected_file)
            .map(|file| file.path.clone());
        self.review.reconcile(&document);
        self.selected_file = selected_path
            .and_then(|path| document.file_index(&path))
            .unwrap_or(0)
            .min(document.files.len().saturating_sub(1));
        self.projection.expansions = expansions;
        self.projection
            .full_files
            .retain(|path| document.file_index(path).is_some());
        if self
            .pending_source_restore
            .as_ref()
            .is_some_and(|location| document.file_index(&location.path).is_none())
        {
            self.pending_source_restore = None;
        }
        self.document = document;
        self.rebuild();
    }

    pub fn set_view_mode(&mut self, mode: ViewMode) -> bool {
        if self.view_mode == mode {
            return false;
        }
        let previous = self.layout();
        self.view_mode = mode;
        if self.layout() != previous {
            self.rebuild();
        }
        true
    }

    pub fn cycle_view_mode(&mut self) -> bool {
        self.set_view_mode(self.view_mode.next())
    }

    pub fn set_split_when_auto(&mut self, split: bool) -> bool {
        if self.split_when_auto == split {
            return false;
        }
        let previous = self.layout();
        self.split_when_auto = split;
        if self.layout() == previous {
            return false;
        }
        self.rebuild();
        true
    }

    pub fn clear_review(&mut self) {
        self.review.clear();
        self.draft = None;
    }

    pub fn select_file(&mut self, index: usize) -> bool {
        if index >= self.document.files.len() {
            return false;
        }
        self.pending_source_restore = None;
        self.selected_file = index;
        self.draft = None;
        self.select_first_row();
        true
    }

    pub fn move_file(&mut self, delta: isize) -> Option<usize> {
        if self.document.files.is_empty() {
            return None;
        }
        let index = offset(self.selected_file, delta, self.document.files.len() - 1);
        self.select_file(index).then_some(index)
    }

    pub fn move_row(&mut self, delta: isize) {
        self.pending_source_restore = None;
        let Some(range) = self.selected_file_range() else {
            return;
        };
        let mut current = self.selected_row;
        if !self.presentation.is_navigable(current) {
            let Some(first) = self.presentation.first_navigable(range.clone()) else {
                return;
            };
            current = first;
            if delta <= 0 {
                self.selected_row = current;
                return;
            }
        }
        for _ in 0..delta.unsigned_abs() {
            let Some(next) = self
                .presentation
                .step_navigable(current, delta.is_negative(), &range)
            else {
                break;
            };
            current = next;
        }
        self.selected_row = current;
        self.normalize_selected_side();
    }

    pub fn select_boundary(&mut self, end: bool) {
        self.pending_source_restore = None;
        let Some(range) = self.selected_file_range() else {
            return;
        };
        let selected = if end {
            self.presentation.last_navigable(range)
        } else {
            self.presentation.first_navigable(range)
        };
        if let Some(index) = selected {
            self.selected_row = index;
            self.normalize_selected_side();
        }
    }

    pub fn select_row(&mut self, index: usize) -> bool {
        if !self.presentation.is_navigable(index) {
            return false;
        }
        self.pending_source_restore = None;
        self.selected_row = index;
        self.normalize_selected_side();
        true
    }

    pub fn move_hunk(&mut self, delta: isize) -> bool {
        self.pending_source_restore = None;
        let Some(file) = self.document.files.get(self.selected_file) else {
            return false;
        };
        if file.hunks.is_empty() {
            return false;
        }
        let current = self
            .presentation
            .row(self.selected_row)
            .and_then(|row| row.hunk_index)
            .unwrap_or(0);
        let target = offset(current, delta, file.hunks.len() - 1);
        let Some(range) = self.presentation.hunk_range(self.selected_file, target) else {
            return false;
        };
        self.selected_row = self
            .presentation
            .first_navigable(range)
            .unwrap_or(self.selected_row);
        self.normalize_selected_side();
        true
    }

    pub fn set_selected_side(&mut self, side: DiffSide) -> bool {
        self.pending_source_restore = None;
        let available = self
            .selected_row()
            .and_then(|index| self.presentation.row(index))
            .is_some_and(|row| row.cell(side).is_some());
        if available {
            self.selected_side = side;
        }
        available
    }

    #[must_use]
    pub fn selected_presented_row(&self) -> Option<&PresentedRow> {
        self.presentation.row(self.selected_row)
    }

    #[must_use]
    pub fn selected_cell(&self) -> Option<&PresentedCell> {
        self.selected_presented_row()?
            .preferred_cell(self.selected_side)
    }

    #[must_use]
    pub fn selected_anchor(&self) -> Option<LineAnchor> {
        let row = self.selected_presented_row()?;
        self.presentation
            .cell_anchor(row, row.preferred_cell(self.selected_side)?)
    }

    #[must_use]
    pub fn comment_id_at_selection(&self) -> Option<u64> {
        let anchor = self.selected_anchor()?;
        self.review
            .comments_for_anchor(&anchor)
            .next_back()
            .map(|comment| comment.id)
    }

    #[must_use]
    pub fn last_comment_id(&self) -> Option<u64> {
        self.review.comments().last().map(|comment| comment.id)
    }

    pub fn begin_draft(&mut self, editing: Option<u64>) -> bool {
        if editing.is_some_and(|id| self.review.comment(id).is_none()) {
            return false;
        }
        let Some(anchor) = self.selected_anchor() else {
            return false;
        };
        let line_text = self
            .selected_cell()
            .map_or_else(String::new, |cell| cell.text.to_string());
        let body = editing
            .and_then(|id| self.review.comment(id))
            .map_or_else(String::new, |comment| comment.body.clone());
        self.draft = Some(CommentDraft::new(anchor, line_text, body, editing));
        true
    }

    pub fn edit_comment_at_selection(&mut self) -> bool {
        self.comment_id_at_selection()
            .is_some_and(|id| self.begin_draft(Some(id)))
    }

    pub fn undo_last_comment(&mut self) -> bool {
        self.last_comment_id()
            .is_some_and(|id| self.review.remove_comment(id).is_some())
    }

    pub fn cancel_draft(&mut self) {
        self.draft = None;
    }

    pub fn submit_draft(&mut self) -> Option<u64> {
        let draft = self.draft.take()?;
        if draft.body.trim().is_empty() {
            return None;
        }
        Some(match draft.editing {
            Some(id) => {
                if !self.review.edit_comment(id, draft.body) {
                    return None;
                }
                id
            }
            None => self
                .review
                .add_comment_with_context(draft.anchor, draft.line_text, draft.body),
        })
    }

    pub fn delete_comment_at_selection(&mut self) -> bool {
        let Some(id) = self.comment_id_at_selection() else {
            return false;
        };
        self.review.remove_comment(id).is_some()
    }

    #[must_use]
    pub fn submission(&self) -> ReviewSubmission {
        self.review.submission()
    }

    fn rebuild(&mut self) {
        let source = self
            .pending_source_restore
            .take()
            .or_else(|| self.selected_source_line());
        let selected_id = self.presentation.row(self.selected_row).map(|row| row.id);
        let anchor = self.selected_anchor();
        let draft_anchor = self.draft.as_ref().map(|draft| draft.anchor.clone());
        self.presentation = DiffPresentation::with_projection(
            self.document.clone(),
            PresentationOptions {
                view_mode: self.view_mode,
                split_when_auto: self.split_when_auto,
                include_file_headers: self.options.include_file_headers,
            },
            &self.projection,
        );
        self.projection_revision = self.projection_revision.wrapping_add(1);
        let source_row = source
            .as_ref()
            .and_then(|location| self.presentation.row_showing_source(location));
        if source_row.is_none() {
            self.pending_source_restore.clone_from(&source);
        }
        let restored = source_row
            .or_else(|| {
                selected_id.and_then(|id| {
                    self.presentation
                        .rows(0..self.presentation.row_count())
                        .iter()
                        .position(|row| row.id == id)
                })
            })
            .or_else(|| {
                anchor
                    .as_ref()
                    .and_then(|anchor| self.presentation.row_showing_anchor(anchor))
            });
        if let Some(row) = restored {
            self.selected_row = row;
            self.selected_file = self
                .presentation
                .row(row)
                .map_or(self.selected_file, |row| row.file_index);
        } else {
            self.select_first_row();
        }
        let draft_is_current = draft_anchor.as_ref().is_none_or(|anchor| {
            self.presentation
                .row_showing_anchor(anchor)
                .and_then(|row_index| self.presentation.row(row_index))
                .and_then(|row| {
                    row.cells()
                        .find(|cell| {
                            cell.patch_source
                                .is_some_and(|source| source.side == anchor.side)
                                && cell.line_number() == anchor.line_number()
                        })
                        .and_then(|cell| self.presentation.cell_anchor(row, cell))
                })
                .is_some_and(|current| current == *anchor)
        });
        if !draft_is_current {
            self.draft = None;
        }
        self.normalize_selected_side();
    }

    fn select_first_row(&mut self) {
        match self.selected_file_range() {
            Some(range) => {
                self.selected_row = self
                    .presentation
                    .first_navigable(range.clone())
                    .unwrap_or(range.start);
            }
            None => self.selected_row = 0,
        }
        self.normalize_selected_side();
    }

    fn normalize_selected_side(&mut self) {
        let Some(row) = self.presentation.row(self.selected_row) else {
            return;
        };
        if row.cell(self.selected_side).is_none()
            && row.cell(self.selected_side.opposite()).is_some()
        {
            self.selected_side = self.selected_side.opposite();
        }
    }
}

fn offset(value: usize, delta: isize, maximum: usize) -> usize {
    if delta.is_negative() {
        value.saturating_sub(delta.unsigned_abs())
    } else {
        value.saturating_add(delta.unsigned_abs()).min(maximum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiffDocument, FileDiff, testing::DocumentBuilder};

    fn session() -> ReviewSession {
        ReviewSession::new(Arc::new(DiffDocument {
            repo_root: "/repo".into(),
            files: vec![
                FileDiff::from_texts("a.rs", "one\ntwo\n", "ONE\nTWO\n").unwrap(),
                FileDiff::from_texts("b.rs", "keep\n", "kept\n").unwrap(),
            ],
        }))
    }

    #[test]
    fn selection_starts_on_a_commentable_row() {
        let session = session();
        assert_eq!(session.selected_file(), Some(0));
        assert!(session.selected_anchor().is_some());
    }

    #[test]
    fn navigation_clamps_at_both_ends() {
        let mut session = session();
        let first = session.selected_row().unwrap();
        session.move_row(-5);
        assert_eq!(session.selected_row(), Some(first));
        session.move_row(1_000);
        let last = session.selected_row().unwrap();
        session.move_row(1_000);
        assert_eq!(session.selected_row(), Some(last));
        assert!(session.selected_anchor().is_some());
    }

    #[test]
    fn file_movement_resets_the_row_and_draft() {
        let mut session = session();
        assert!(session.begin_draft(None));
        assert_eq!(session.move_file(1), Some(1));
        assert!(session.draft().is_none());
        assert_eq!(session.move_file(9), Some(1));
        assert_eq!(session.move_file(-9), Some(0));
    }

    #[test]
    fn split_selection_can_target_the_removed_side() {
        let mut session = session();
        session.set_view_mode(ViewMode::Split);
        assert_eq!(session.layout(), Layout::Split);
        assert!(session.set_selected_side(DiffSide::Old));
        assert_eq!(
            session.selected_anchor().map(|anchor| anchor.side),
            Some(DiffSide::Old)
        );
        assert!(session.set_selected_side(DiffSide::New));
        assert_eq!(
            session.selected_anchor().map(|anchor| anchor.side),
            Some(DiffSide::New)
        );
    }

    #[test]
    fn selection_side_follows_cells_available_on_each_row() {
        let document = Arc::new(DiffDocument {
            repo_root: "/repo".into(),
            files: vec![FileDiff::from_texts("a.rs", "old\n", "new\nextra\n").unwrap()],
        });
        let mut session = ReviewSession::new(document);
        session.set_view_mode(ViewMode::Split);
        assert!(session.set_selected_side(DiffSide::Old));
        session.move_row(1);
        assert_eq!(session.selected_side(), DiffSide::New);
        assert!(!session.set_selected_side(DiffSide::Old));
        assert_eq!(session.selected_side(), DiffSide::New);
    }

    #[test]
    fn auto_mode_only_rebuilds_when_the_layout_changes() {
        let mut session = session();
        assert!(session.set_split_when_auto(true));
        assert_eq!(session.layout(), Layout::Split);
        assert!(!session.set_split_when_auto(true));
        assert!(session.set_view_mode(ViewMode::Split));
        assert!(!session.set_split_when_auto(false));
        assert_eq!(session.layout(), Layout::Split);
    }

    #[test]
    fn drafts_round_trip_through_the_review() {
        let mut session = session();
        assert!(session.begin_draft(None));
        session.draft_mut().unwrap().insert("please fix");
        let id = session.submit_draft().unwrap();
        assert_eq!(session.review().len(), 1);
        assert_eq!(session.comment_id_at_selection(), Some(id));

        assert!(session.begin_draft(Some(id)));
        assert_eq!(session.draft().unwrap().body(), "please fix");
        session.draft_mut().unwrap().insert(" now");
        assert_eq!(session.submit_draft(), Some(id));
        assert_eq!(session.review().comment(id).unwrap().body, "please fix now");
        assert_eq!(session.review().len(), 1);

        assert!(session.delete_comment_at_selection());
        assert!(session.review().is_empty());
    }

    #[test]
    fn a_blank_draft_adds_nothing() {
        let mut session = session();
        assert!(session.begin_draft(None));
        session.draft_mut().unwrap().insert("   ");
        assert_eq!(session.submit_draft(), None);
        assert!(session.review().is_empty());
    }

    #[test]
    fn draft_editing_is_utf8_safe() {
        let mut session = session();
        session.begin_draft(None);
        let draft = session.draft_mut().unwrap();
        draft.insert("a界b");
        draft.move_cursor_left();
        draft.delete_before_cursor();
        assert_eq!(draft.body(), "ab");
        draft.move_cursor_to_start();
        draft.delete_at_cursor();
        assert_eq!(draft.body(), "b");
        draft.move_cursor_to_end();
        assert_eq!(draft.cursor(), 1);
    }

    #[allow(clippy::format_collect)]
    fn full_file_session() -> ReviewSession {
        let old = (1..=60)
            .map(|line| format!("old {line}\n"))
            .collect::<String>();
        let new = old.replace("old 31\n", "new 31\n");
        ReviewSession::new(
            DocumentBuilder::new()
                .changed_with_hunk_window("a.rs", &old, &new, 28..=34)
                .build(),
        )
    }

    #[test]
    fn eager_full_file_projection_keeps_expanded_lines_non_commentable() {
        let mut session = full_file_session();
        let selected = session.selected_source_line().unwrap();
        assert!(session.toggle_full_file());
        assert_eq!(session.selected_source_line(), Some(selected));
        let expanded = session
            .presentation()
            .rows(0..session.presentation().row_count())
            .iter()
            .position(|row| row.kind == crate::RowKind::ExpandedContext)
            .unwrap();
        assert!(session.select_row(expanded));
        let expanded_location = session.selected_source_line().unwrap();
        assert!(session.selected_anchor().is_none());
        assert!(!session.begin_draft(None));
        assert!(session.toggle_full_file());
        assert!(session.toggle_full_file());
        assert_eq!(session.selected_source_line(), Some(expanded_location));
    }

    #[test]
    fn document_replacement_preserves_an_existing_patch_selection_and_draft() {
        let mut session = session();
        session.move_row(2);
        let anchor = session.selected_anchor().unwrap();
        assert!(session.begin_draft(None));
        session.draft_mut().unwrap().insert("still editing");
        let document = session.document().clone();
        session.set_document(document);
        assert_eq!(session.selected_anchor(), Some(anchor));
        assert_eq!(
            session.draft().map(CommentDraft::body),
            Some("still editing")
        );
    }

    #[test]
    fn document_replacement_drops_a_draft_when_content_changes_at_the_same_line() {
        let mut session = session();
        assert!(session.begin_draft(None));
        session.draft_mut().unwrap().insert("stale draft");
        let replacement = Arc::new(DiffDocument {
            repo_root: "/repo".into(),
            files: vec![
                FileDiff::from_texts("a.rs", "different\ntwo\n", "CHANGED\nTWO\n").unwrap(),
            ],
        });
        session.set_document(replacement);
        assert!(session.draft().is_none());
    }

    #[allow(clippy::format_collect)]
    fn windowed_source(changed_lines: &[usize]) -> (String, String) {
        let old = (1..=60)
            .map(|line| format!("old {line}\n"))
            .collect::<String>();
        let new = changed_lines.iter().fold(old.clone(), |text, line| {
            text.replace(&format!("old {line}\n"), &format!("new {line}\n"))
        });
        (old, new)
    }

    fn windowed_fixture(new_text: &str) -> Arc<DiffDocument> {
        let (old, _) = windowed_source(&[]);
        DocumentBuilder::new()
            .changed_with_hunk_window("a.rs", &old, new_text, 28..=34)
            .build()
    }

    fn expanded_row_count(session: &ReviewSession) -> usize {
        let presentation = session.presentation();
        presentation
            .rows(0..presentation.row_count())
            .iter()
            .filter(|row| row.kind == crate::RowKind::ExpandedContext)
            .count()
    }

    fn reveal_from_first_code_row(session: &mut ReviewSession) -> usize {
        let code_row = (0..session.presentation().row_count())
            .find(|index| {
                session
                    .presentation()
                    .row(*index)
                    .is_some_and(|row| row.kind == crate::RowKind::Code)
            })
            .expect("a code row");
        assert!(session.select_row(code_row));
        assert!(session.reveal_selected_gap(RevealAmount::Step));
        let expanded = expanded_row_count(session);
        assert!(expanded > 0, "revealing must expand context");
        expanded
    }

    #[test]
    fn from_texts_documents_capture_both_sides_and_reveal_unchanged_context() {
        let old = (1..=80).fold(String::new(), |mut text, line| {
            use std::fmt::Write;
            let _ = writeln!(text, "line {line}");
            text
        });
        let new = old.replace("line 40\n", "changed 40\n");
        let document = DiffDocument::from_texts([
            ("src/changed.rs", old.as_str(), new.as_str()),
            ("src/added.rs", "", "fn main() {}\n"),
        ])
        .unwrap();
        let changed = &document.files[0];
        assert_eq!(changed.hunks.len(), 1);
        assert!(changed.hunks[0].lines.len() <= 8);
        assert_eq!(
            changed.source_document(DiffSide::Old).unwrap().line_count(),
            80
        );
        assert_eq!(
            document.files[1].source_unavailable(DiffSide::Old),
            Some(&crate::SourceUnavailable::Absent)
        );

        let mut session = ReviewSession::new(Arc::new(document));
        reveal_from_first_code_row(&mut session);
        let presentation = session.presentation();
        let gap = (0..presentation.row_count())
            .find_map(|index| presentation.gap_info(index))
            .expect("a remaining collapsed gap");
        assert_eq!(gap.unavailable, None);
    }

    #[test]
    fn snapshot_replacement_retains_gap_expansions_through_an_index_shift() {
        let (_, new) = windowed_source(&[31]);
        let fixture = windowed_fixture(&new);
        let mut session = ReviewSession::new(fixture.clone());
        let expanded = reveal_from_first_code_row(&mut session);

        let (old, _) = windowed_source(&[]);
        let replacement = DocumentBuilder::new()
            .changed("inserted.rs", "one\n", "two\n")
            .changed_with_hunk_window("a.rs", &old, &new, 28..=34)
            .build();
        session.set_document(replacement);

        assert_eq!(session.selected_file(), Some(1));
        assert_eq!(expanded_row_count(&session), expanded);
    }

    #[test]
    fn snapshot_replacement_keeps_selection_in_expanded_context() {
        let (_, new) = windowed_source(&[31]);
        let fixture = windowed_fixture(&new);
        let mut session = ReviewSession::new(fixture.clone());
        reveal_from_first_code_row(&mut session);
        let expanded = session
            .presentation()
            .rows(0..session.presentation().row_count())
            .iter()
            .position(|row| row.kind == crate::RowKind::ExpandedContext)
            .expect("expanded context exists");
        assert!(session.select_row(expanded));
        let source = session
            .selected_source_line()
            .expect("expanded row has source location");
        session.set_document(windowed_fixture(&new));
        assert_eq!(session.selected_source_line(), Some(source));
        assert_eq!(
            session
                .presentation()
                .row(session.selected_row().unwrap())
                .unwrap()
                .kind,
            crate::RowKind::ExpandedContext
        );
    }

    #[test]
    fn snapshot_replacement_drops_gap_expansions_when_a_side_changes() {
        let (_, new) = windowed_source(&[31]);
        let fixture = windowed_fixture(&new);
        let mut session = ReviewSession::new(fixture.clone());
        reveal_from_first_code_row(&mut session);

        let (_, edited) = windowed_source(&[31, 55]);
        let replacement = windowed_fixture(&edited);
        assert_eq!(
            replacement.files[0].hunks, fixture.files[0].hunks,
            "the retained window must stay equal so only source content differs"
        );
        session.set_document(replacement);

        assert_eq!(expanded_row_count(&session), 0);
    }

    #[test]
    fn snapshot_replacement_keeps_a_draft_when_another_file_changes() {
        let fixture = DocumentBuilder::new()
            .changed("a.rs", "one\ntwo\n", "ONE\nTWO\n")
            .changed("b.rs", "keep\n", "kept\n")
            .build();
        let mut session = ReviewSession::new(fixture.clone());
        assert!(session.begin_draft(None));
        session.draft_mut().unwrap().insert("still editing");

        let replacement = DocumentBuilder::new()
            .changed("a.rs", "one\ntwo\n", "ONE\nTWO\n")
            .changed("b.rs", "keep\n", "changed\n")
            .build();
        session.set_document(replacement);

        assert_eq!(
            session.draft().map(CommentDraft::body),
            Some("still editing")
        );
    }

    #[test]
    fn replacing_the_document_keeps_the_selected_path() {
        let mut session = session();
        session.move_file(1);
        let document = session.document().clone();
        session.set_document(document);
        assert_eq!(session.selected_file(), Some(1));
    }
}
