//! Selection, draft editing, and decisions for a Markdown review.

use crate::{
    MarkdownAnchor, MarkdownCommentContext, MarkdownDocument, MarkdownReview,
    MarkdownReviewDecision, MarkdownReviewError, MarkdownReviewEvent, MarkdownReviewSubmission,
    MarkdownTarget, MarkdownTargetId,
};
use std::sync::Arc;

/// An editable UTF-8-safe Markdown comment draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownCommentDraft {
    target: MarkdownTargetId,
    anchor: MarkdownAnchor,
    context: MarkdownCommentContext,
    body: String,
    cursor: usize,
    editing: Option<u64>,
}

impl MarkdownCommentDraft {
    fn new(
        target: MarkdownTargetId,
        anchor: MarkdownAnchor,
        context: MarkdownCommentContext,
        body: String,
        editing: Option<u64>,
    ) -> Self {
        Self {
            target,
            anchor,
            context,
            cursor: body.len(),
            body,
            editing,
        }
    }

    #[must_use]
    pub const fn target(&self) -> MarkdownTargetId {
        self.target
    }

    #[must_use]
    pub const fn target_id(&self) -> MarkdownTargetId {
        self.target
    }

    #[must_use]
    pub const fn anchor(&self) -> &MarkdownAnchor {
        &self.anchor
    }

    #[must_use]
    pub const fn context(&self) -> &MarkdownCommentContext {
        &self.context
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

/// State for reviewing one parsed Markdown document.
#[derive(Debug, Clone)]
pub struct MarkdownReviewSession {
    document: Arc<MarkdownDocument>,
    review: MarkdownReview,
    selected_target: Option<MarkdownTargetId>,
    draft: Option<MarkdownCommentDraft>,
}

impl MarkdownReviewSession {
    #[must_use]
    pub fn new(document: Arc<MarkdownDocument>) -> Self {
        let selected_target = document.targets().first().map(|target| target.id);
        Self {
            document,
            review: MarkdownReview::default(),
            selected_target,
            draft: None,
        }
    }

    #[must_use]
    pub const fn document(&self) -> &Arc<MarkdownDocument> {
        &self.document
    }

    #[must_use]
    pub const fn review(&self) -> &MarkdownReview {
        &self.review
    }

    pub const fn review_mut(&mut self) -> &mut MarkdownReview {
        &mut self.review
    }

    #[must_use]
    pub const fn selected_target(&self) -> Option<MarkdownTargetId> {
        self.selected_target
    }

    #[must_use]
    pub fn selected_target_info(&self) -> Option<&MarkdownTarget> {
        self.selected_target.and_then(|id| self.document.target(id))
    }

    #[must_use]
    pub fn selected_anchor(&self) -> Option<MarkdownAnchor> {
        self.selected_target
            .and_then(|id| self.document.anchor_for_target(id))
    }

    #[must_use]
    pub const fn draft(&self) -> Option<&MarkdownCommentDraft> {
        self.draft.as_ref()
    }

    pub const fn draft_mut(&mut self) -> Option<&mut MarkdownCommentDraft> {
        self.draft.as_mut()
    }

    /// Selects a target from this document.
    pub fn select_target(&mut self, target: MarkdownTargetId) -> bool {
        if self.document.target(target).is_none() {
            return false;
        }
        self.selected_target = Some(target);
        self.draft = None;
        true
    }

    /// Alias for [`Self::select_target`] useful to frontend adapters.
    pub fn select(&mut self, target: MarkdownTargetId) -> bool {
        self.select_target(target)
    }

    /// Moves one target forward or backward.
    pub fn move_by(&mut self, delta: isize) -> Option<MarkdownTargetId> {
        self.move_target(delta)
    }

    /// Returns the most recently added comment at the current selection.
    #[must_use]
    pub fn last_comment_id(&self) -> Option<u64> {
        self.review.most_recent_comment_id()
    }

    /// Returns comments attached to the current selection.
    #[must_use]
    pub fn selected_comments(&self) -> std::vec::IntoIter<&crate::MarkdownReviewComment> {
        self.selected_target.map_or_else(
            || Vec::new().into_iter(),
            |target| self.review.comments_for_target(&self.document, target),
        )
    }

    /// Moves through commentable targets, clamping at either end.
    pub fn move_target(&mut self, delta: isize) -> Option<MarkdownTargetId> {
        let count = self.document.targets().len();
        if count == 0 {
            self.selected_target = None;
            return None;
        }
        let current = self
            .selected_target
            .map_or(0, MarkdownTargetId::index)
            .min(count - 1);
        let index = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta.unsigned_abs()).min(count - 1)
        };
        self.selected_target = Some(self.document.targets()[index].id);
        self.draft = None;
        self.selected_target
    }

    /// Selects the first or last target.
    pub fn select_boundary(&mut self, last: bool) -> Option<MarkdownTargetId> {
        let target = if last {
            self.document.targets().last()
        } else {
            self.document.targets().first()
        }
        .map(|target| target.id);
        self.selected_target = target;
        self.draft = None;
        target
    }

    pub fn select_first_target(&mut self) -> Option<MarkdownTargetId> {
        self.select_boundary(false)
    }

    pub fn select_last_target(&mut self) -> Option<MarkdownTargetId> {
        self.select_boundary(true)
    }

    /// Selects the next heading after the current target.
    pub fn next_heading(&mut self) -> Option<MarkdownTargetId> {
        self.move_heading(false)
    }

    /// Selects the previous heading before the current target.
    pub fn previous_heading(&mut self) -> Option<MarkdownTargetId> {
        self.move_heading(true)
    }

    fn move_heading(&mut self, previous: bool) -> Option<MarkdownTargetId> {
        let current = self.selected_target.map(MarkdownTargetId::index);
        let headings = self.document.outline();
        let heading = if previous {
            headings
                .iter()
                .rev()
                .find(|heading| current.is_none_or(|index| heading.target_id.index() < index))
        } else {
            headings
                .iter()
                .find(|heading| current.is_none_or(|index| heading.target_id.index() > index))
        }?;
        self.selected_target = Some(heading.target_id);
        self.draft = None;
        self.selected_target
    }

    /// Begins a new draft on the current target, or edits an existing comment.
    pub fn begin_draft(&mut self, editing: Option<u64>) -> bool {
        let Some(target) = self.selected_target else {
            return false;
        };
        let Some(anchor) = self.document.anchor_for_target(target) else {
            return false;
        };
        let Some(context) = MarkdownCommentContext::from_target(&self.document, target, &anchor)
        else {
            return false;
        };
        let body = match editing {
            Some(id) => {
                let Some(comment) = self.review.comment(id) else {
                    return false;
                };
                // Editing is intentionally tied to the selected target. This prevents
                // an editor from silently changing an unrelated comment after a move.
                if comment.anchor != anchor {
                    return false;
                }
                comment.body.clone()
            }
            None => String::new(),
        };
        self.draft = Some(MarkdownCommentDraft::new(
            target, anchor, context, body, editing,
        ));
        true
    }

    pub fn cancel_draft(&mut self) {
        self.draft = None;
    }

    /// Saves the active draft, returning its comment ID. Blank drafts are discarded.
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
                .add_comment_with_context(draft.anchor, draft.context, draft.body),
        })
    }

    #[must_use]
    pub fn comment_id_at_selection(&self) -> Option<u64> {
        let anchor = self.selected_anchor()?;
        self.review
            .comments_for_anchor(&anchor)
            .next_back()
            .map(|comment| comment.id)
    }

    pub fn edit_comment_at_selection(&mut self) -> bool {
        self.begin_draft(self.comment_id_at_selection())
    }

    pub fn delete_comment_at_selection(&mut self) -> bool {
        let Some(id) = self.comment_id_at_selection() else {
            return false;
        };
        self.review.remove_comment(id).is_some()
    }

    /// Removes the most recently added comment, if there is one.
    pub fn undo_last_comment(&mut self) -> bool {
        let Some(id) = self.review.most_recent_comment_id() else {
            return false;
        };
        self.review.remove_comment(id).is_some()
    }

    pub fn clear_review(&mut self) {
        self.review.clear();
        self.draft = None;
    }

    /// Replaces the document, reconciles comments, and preserves selection when possible.
    pub fn replace_document(&mut self, document: Arc<MarkdownDocument>) {
        let selected_anchor = self.selected_anchor();
        self.review.reconcile(&document);
        self.document = document;
        self.draft = None;
        self.selected_target = selected_anchor
            .and_then(|anchor| self.document.resolve_anchor(&anchor))
            .or_else(|| self.document.targets().first().map(|target| target.id));
    }

    /// Alias for [`Self::replace_document`] used by host adapters.
    pub fn set_document(&mut self, document: Arc<MarkdownDocument>) {
        self.replace_document(document);
    }

    /// Builds a submission for an explicit decision.
    #[must_use]
    pub fn submission(&self, decision: MarkdownReviewDecision) -> MarkdownReviewSubmission {
        self.review.submission(decision)
    }

    /// Builds an explicit submission event for either decision.
    ///
    /// # Errors
    ///
    /// Returns an error when a comment body is blank.
    pub fn submit(
        &self,
        decision: MarkdownReviewDecision,
    ) -> Result<MarkdownReviewEvent, MarkdownReviewError> {
        Ok(MarkdownReviewEvent::Submit(
            self.review.try_submission(decision)?,
        ))
    }

    /// Builds an explicit approval event.
    ///
    /// # Errors
    ///
    /// Returns an error when a comment body is blank.
    pub fn approve(&self) -> Result<MarkdownReviewEvent, MarkdownReviewError> {
        self.submit(MarkdownReviewDecision::Approved)
    }

    /// Builds an explicit request-changes event.
    ///
    /// # Errors
    ///
    /// Returns an error when a comment body is blank.
    pub fn request_changes(&self) -> Result<MarkdownReviewEvent, MarkdownReviewError> {
        self.submit(MarkdownReviewDecision::ChangesRequested)
    }

    /// Builds an explicit approval submission.
    ///
    /// # Errors
    ///
    /// Returns an error when a comment body is blank.
    pub fn approve_submission(&self) -> Result<MarkdownReviewSubmission, MarkdownReviewError> {
        self.review.try_submission(MarkdownReviewDecision::Approved)
    }

    /// Builds an explicit request-changes submission.
    ///
    /// # Errors
    ///
    /// Returns an error when a comment body is blank.
    pub fn request_changes_submission(
        &self,
    ) -> Result<MarkdownReviewSubmission, MarkdownReviewError> {
        self.review
            .try_submission(MarkdownReviewDecision::ChangesRequested)
    }

    #[must_use]
    pub fn cancel(&self) -> MarkdownReviewEvent {
        MarkdownReviewEvent::Cancel
    }

    #[must_use]
    pub fn copy_formatted(&self, decision: MarkdownReviewDecision) -> MarkdownReviewEvent {
        MarkdownReviewEvent::CopyFormatted(self.review.submission(decision).formatted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MarkdownDocument, MarkdownTargetKind};

    fn session(source: &str) -> MarkdownReviewSession {
        MarkdownReviewSession::new(Arc::new(MarkdownDocument::parse_with_metadata(
            Some("plan.md".into()),
            None,
            source,
        )))
    }

    #[test]
    fn navigates_targets_and_headings() {
        let mut session = session("# One\n\nfirst\n\n## Two\n\nsecond\n");
        assert_eq!(
            session.selected_target_info().unwrap().kind,
            MarkdownTargetKind::Heading
        );
        assert_eq!(session.next_heading().unwrap().index(), 2);
        assert_eq!(session.next_heading(), None);
        assert_eq!(session.previous_heading().unwrap().index(), 0);
        assert_eq!(session.select_last_target().unwrap().index(), 3);
        assert_eq!(session.select_first_target().unwrap().index(), 0);
    }

    #[test]
    fn drafts_are_utf8_safe_and_support_edit_delete_undo() {
        let mut session = session("paragraph\n");
        assert!(session.begin_draft(None));
        session.draft_mut().unwrap().insert("a界b");
        session.draft_mut().unwrap().move_cursor_left();
        session.draft_mut().unwrap().delete_before_cursor();
        assert_eq!(session.draft().unwrap().body(), "ab");
        let id = session.submit_draft().unwrap();
        assert_eq!(session.comment_id_at_selection(), Some(id));
        assert!(session.edit_comment_at_selection());
        session.draft_mut().unwrap().insert(" revised");
        assert_eq!(session.submit_draft(), Some(id));
        assert!(session.delete_comment_at_selection());
        assert!(session.begin_draft(None));
        session.draft_mut().unwrap().insert("undo");
        session.submit_draft();
        assert!(session.undo_last_comment());
    }

    #[test]
    fn replacement_preserves_selection_and_marks_removed_comments_outdated() {
        let mut session = session("# Plan\n\nKeep\n\nRemove\n");
        session.move_target(2);
        let selected = session.selected_target();
        session.begin_draft(None);
        session.draft_mut().unwrap().insert("note");
        session.submit_draft();
        session.replace_document(Arc::new(MarkdownDocument::parse_with_metadata(
            Some("new.md".into()),
            None,
            "# Plan\n\nInserted\n\nKeep\n",
        )));
        assert_ne!(session.selected_target(), None);
        assert_eq!(session.review().outdated_count(), 1);
        assert_eq!(session.draft(), None);
        assert_ne!(session.selected_target(), selected);
    }

    #[test]
    fn decision_events_are_distinct() {
        let session = session("text\n");
        let approved = session.approve().unwrap();
        let requested = session.request_changes().unwrap();
        assert!(
            matches!(approved, MarkdownReviewEvent::Submit(submission) if submission.decision == MarkdownReviewDecision::Approved)
        );
        assert!(
            matches!(requested, MarkdownReviewEvent::Submit(submission) if submission.decision == MarkdownReviewDecision::ChangesRequested)
        );
        assert!(matches!(session.cancel(), MarkdownReviewEvent::Cancel));
    }
}
