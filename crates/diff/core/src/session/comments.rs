use super::ReviewSession;
use crate::{AgentFeedbackOptions, LineAnchor, Review, ReviewSubmission};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentDraft {
    pub(super) anchor: LineAnchor,
    line_text: String,
    body: String,
    cursor: usize,
    editing: Option<u64>,
}

impl CommentDraft {
    fn new(anchor: LineAnchor, line_text: String, body: String, editing: Option<u64>) -> Self {
        Self { anchor, line_text, cursor: body.len(), body, editing }
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
        self.body[..self.cursor].char_indices().next_back().map_or(0, |(offset, _)| offset)
    }

    fn next_boundary(&self) -> usize {
        self.body[self.cursor..].char_indices().nth(1).map_or(self.body.len(), |(offset, _)| self.cursor + offset)
    }
}

impl ReviewSession {
    #[must_use]
    pub const fn review(&self) -> &Review {
        &self.review
    }

    pub fn review_mut(&mut self) -> &mut Review {
        self.annotations_changed();
        &mut self.review
    }

    #[must_use]
    pub const fn annotations_revision(&self) -> u64 {
        self.annotations_revision
    }

    pub(super) const fn annotations_changed(&mut self) {
        self.annotations_revision = self.annotations_revision.wrapping_add(1);
    }

    pub(super) fn set_draft(&mut self, draft: Option<CommentDraft>) {
        if self.draft.is_some() || draft.is_some() {
            self.annotations_changed();
        }
        self.draft = draft;
    }

    #[must_use]
    pub const fn draft(&self) -> Option<&CommentDraft> {
        self.draft.as_ref()
    }

    pub fn draft_mut(&mut self) -> Option<&mut CommentDraft> {
        if self.draft.is_some() {
            self.annotations_changed();
        }
        self.draft.as_mut()
    }

    pub fn clear_review(&mut self) {
        self.review_mut().clear();
        self.set_draft(None);
    }

    #[must_use]
    pub fn comment_id_at_selection(&self) -> Option<u64> {
        let anchor = self.selected_anchor()?;
        self.review.comments_for_anchor(&anchor).next_back().map(|comment| comment.id)
    }

    #[must_use]
    pub fn last_comment_id(&self) -> Option<u64> {
        self.review.comments().next_back().map(|comment| comment.id)
    }

    pub fn begin_draft(&mut self, editing: Option<u64>) -> bool {
        if editing.is_some_and(|id| self.review.comment(id).is_none()) {
            return false;
        }
        let Some(anchor) = self.selected_anchor() else {
            return false;
        };
        let line_text = self.selected_cell().map_or_else(String::new, |cell| cell.source_text().to_owned());
        let body =
            editing.and_then(|id| self.review.comment(id)).map_or_else(String::new, |comment| comment.body.clone());
        self.set_draft(Some(CommentDraft::new(anchor, line_text, body, editing)));
        true
    }

    pub fn edit_comment_at_selection(&mut self) -> bool {
        self.comment_id_at_selection().is_some_and(|id| self.begin_draft(Some(id)))
    }

    pub fn undo_last_comment(&mut self) -> bool {
        if self.shows_source() {
            return false;
        }
        self.last_comment_id().is_some_and(|id| self.review_mut().remove_comment(id).is_some())
    }

    pub fn cancel_draft(&mut self) {
        self.set_draft(None);
    }

    pub fn submit_draft(&mut self) -> Option<u64> {
        let draft = self.draft.take()?;
        self.annotations_changed();
        if draft.body.trim().is_empty() {
            return None;
        }
        Some(match draft.editing {
            Some(id) => {
                if !self.review_mut().edit_comment(id, draft.body) {
                    return None;
                }
                id
            }
            None => self.review_mut().add_comment_with_context(draft.anchor, draft.line_text, draft.body),
        })
    }

    pub fn delete_comment_at_selection(&mut self) -> bool {
        let Some(id) = self.comment_id_at_selection() else {
            return false;
        };
        self.review_mut().remove_comment(id).is_some()
    }

    #[must_use]
    pub fn submission(&self) -> ReviewSubmission {
        match self.commits() {
            Some(commits) if self.review.targets_commits() => {
                self.review.submission_with(&AgentFeedbackOptions::for_commits(Arc::clone(commits)))
            }
            _ => self.review.submission(),
        }
    }
}
