use crate::{
    DiffDocument, DiffPresentation, DiffSide, Layout, LineAnchor, PresentationOptions,
    PresentedCell, PresentedRow, Review, ReviewSubmission, ViewMode,
};
use std::{ops::Range, sync::Arc};

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

    pub fn set_document(&mut self, document: Arc<DiffDocument>) {
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
        self.document = document;
        self.draft = None;
        self.rebuild();
        self.select_first_row();
    }

    pub fn set_view_mode(&mut self, mode: ViewMode) -> bool {
        if self.view_mode == mode {
            return false;
        }
        let previous = self.layout();
        self.view_mode = mode;
        if self.layout() != previous {
            self.rebuild();
            self.select_first_row();
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
        self.select_first_row();
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
        let Some(range) = self.selected_file_range() else {
            return;
        };
        let mut current = self.selected_row;
        if !self.presentation.is_commentable(current) {
            let Some(first) = self.presentation.first_commentable(range.clone()) else {
                return;
            };
            current = first;
            if delta <= 0 {
                self.selected_row = current;
                return;
            }
        }
        for _ in 0..delta.unsigned_abs() {
            let Some(next) =
                self.presentation
                    .step_commentable(current, delta.is_negative(), &range)
            else {
                break;
            };
            current = next;
        }
        self.selected_row = current;
        self.normalize_selected_side();
    }

    pub fn select_boundary(&mut self, end: bool) {
        let Some(range) = self.selected_file_range() else {
            return;
        };
        let selected = if end {
            self.presentation.last_commentable(range)
        } else {
            self.presentation.first_commentable(range)
        };
        if let Some(index) = selected {
            self.selected_row = index;
            self.normalize_selected_side();
        }
    }

    pub fn select_row(&mut self, index: usize) -> bool {
        if !self.presentation.is_commentable(index) {
            return false;
        }
        self.selected_row = index;
        self.normalize_selected_side();
        true
    }

    pub fn move_hunk(&mut self, delta: isize) -> bool {
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
            .first_commentable(range)
            .unwrap_or(self.selected_row);
        self.normalize_selected_side();
        true
    }

    pub fn set_selected_side(&mut self, side: DiffSide) -> bool {
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
        self.presentation = DiffPresentation::new(
            self.document.clone(),
            PresentationOptions {
                view_mode: self.view_mode,
                split_when_auto: self.split_when_auto,
                include_file_headers: self.options.include_file_headers,
            },
        );
        self.selected_row = self
            .selected_row
            .min(self.presentation.row_count().saturating_sub(1));
    }

    fn select_first_row(&mut self) {
        match self.selected_file_range() {
            Some(range) => {
                self.selected_row = self
                    .presentation
                    .first_commentable(range.clone())
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
    use crate::{DiffDocument, FileDiff};

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

    #[test]
    fn replacing_the_document_keeps_the_selected_path() {
        let mut session = session();
        session.move_file(1);
        let document = session.document().clone();
        session.set_document(document);
        assert_eq!(session.selected_file(), Some(1));
    }
}
