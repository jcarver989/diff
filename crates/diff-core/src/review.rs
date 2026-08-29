//! Structured review comments, reconciliation, and agent-facing output.

use crate::{DiffDocument, DiffSide, FileStatus, Fingerprint, LineAnchor, PatchLineKind, RepoPath};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
};

/// The captured source context for a comment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentContext {
    pub path: RepoPath,
    pub side: DiffSide,
    pub line_number: Option<usize>,
    pub line_kind: PatchLineKind,
    pub line_text: String,
}

/// A review comment retained even when its source disappears.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewComment {
    pub id: u64,
    pub anchor: LineAnchor,
    pub body: String,
    pub context: CommentContext,
    pub outdated: bool,
}

/// Mutable collection of review comments.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    comments: Vec<ReviewComment>,
    next_id: u64,
}

impl Review {
    /// Adds a comment and returns its stable review-local ID.
    pub fn add_comment(&mut self, anchor: LineAnchor, body: impl Into<String>) -> u64 {
        self.add_comment_with_context(anchor, String::new(), body)
    }

    pub fn add_comment_with_context(
        &mut self,
        anchor: LineAnchor,
        line_text: impl Into<String>,
        body: impl Into<String>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let context = CommentContext {
            path: anchor.path.clone(),
            side: anchor.side,
            line_number: anchor.line_number(),
            line_kind: anchor.kind,
            line_text: line_text.into(),
        };
        self.comments.push(ReviewComment {
            id,
            anchor,
            body: body.into(),
            context,
            outdated: false,
        });
        id
    }

    /// Changes a comment body, returning whether the ID existed.
    pub fn edit_comment(&mut self, id: u64, body: impl Into<String>) -> bool {
        if let Some(comment) = self.comments.iter_mut().find(|comment| comment.id == id) {
            comment.body = body.into();
            return true;
        }
        false
    }

    /// Removes a comment, returning it if present.
    pub fn remove_comment(&mut self, id: u64) -> Option<ReviewComment> {
        let index = self.comments.iter().position(|comment| comment.id == id)?;
        Some(self.comments.remove(index))
    }

    #[must_use]
    pub fn comment(&self, id: u64) -> Option<&ReviewComment> {
        self.comments.iter().find(|comment| comment.id == id)
    }

    /// Returns whether the review has no comments.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.comments.is_empty()
    }

    /// Returns the number of comments.
    #[must_use]
    pub fn len(&self) -> usize {
        self.comments.len()
    }

    #[must_use]
    pub fn outdated_count(&self) -> usize {
        self.comments.iter().filter(|c| c.outdated).count()
    }

    /// Returns all comments, in insertion order.
    #[must_use]
    pub fn comments(&self) -> &[ReviewComment] {
        &self.comments
    }

    /// Returns comments associated with a path.
    pub fn comments_for_file(&self, path: &RepoPath) -> impl Iterator<Item = &ReviewComment> {
        self.comments
            .iter()
            .filter(move |comment| &comment.context.path == path || &comment.anchor.path == path)
    }

    #[must_use]
    pub fn comments_for_anchor<'a>(
        &'a self,
        anchor: &'a LineAnchor,
    ) -> impl DoubleEndedIterator<Item = &'a ReviewComment> {
        self.comments
            .iter()
            .filter(move |comment| &comment.anchor == anchor)
    }

    /// Removes all comments.
    pub fn clear(&mut self) {
        self.comments.clear();
    }

    /// Reattaches comments to a replacement document and marks misses outdated.
    pub fn reconcile(&mut self, document: &DiffDocument) {
        if self.comments.is_empty() {
            return;
        }
        let wanted: HashSet<Fingerprint> = self
            .comments
            .iter()
            .map(|comment| comment.anchor.content_fingerprint)
            .collect();
        let mut candidates: HashMap<Fingerprint, Vec<LineAnchor>> = HashMap::new();
        for file in &document.files {
            for (hunk_index, hunk) in file.hunks.iter().enumerate() {
                for (line_index, line) in hunk.lines.iter().enumerate() {
                    for side in line.kind.sides() {
                        let fingerprint = LineAnchor::content_fingerprint_of(*side, line);
                        if !wanted.contains(&fingerprint) {
                            continue;
                        }
                        if let Some(anchor) =
                            LineAnchor::for_line(file, *side, hunk_index, line_index)
                        {
                            candidates.entry(fingerprint).or_default().push(anchor);
                        }
                    }
                }
            }
        }

        for comment in &mut self.comments {
            let matched = candidates
                .get(&comment.anchor.content_fingerprint)
                .and_then(|bucket| reattach(bucket, &comment.anchor));
            match matched {
                Some(anchor) => {
                    comment.anchor = anchor;
                    comment.outdated = false;
                }
                None => comment.outdated = true,
            }
        }
    }

    #[must_use]
    pub fn submission(&self) -> ReviewSubmission {
        self.submission_with(&AgentFeedbackOptions::default())
    }

    #[must_use]
    pub fn submission_with(&self, options: &AgentFeedbackOptions) -> ReviewSubmission {
        ReviewSubmission {
            comments: self.comments.clone(),
            formatted: format_review(self, options),
        }
    }
}

fn reattach(bucket: &[LineAnchor], previous: &LineAnchor) -> Option<LineAnchor> {
    if let Some(exact) = bucket.iter().find(|anchor| *anchor == previous) {
        return Some(exact.clone());
    }
    let line = previous.line_number().unwrap_or(0);
    bucket
        .iter()
        .filter(|anchor| anchor.addresses_same_side(previous))
        .min_by_key(|anchor| anchor.line_number().unwrap_or(0).abs_diff(line))
        .cloned()
}

/// Structured review data and its deterministic prompt representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewSubmission {
    pub comments: Vec<ReviewComment>,
    pub formatted: String,
}

/// Options controlling feedback wording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFeedbackOptions {
    pub intro: String,
    pub include_outdated: bool,
}

impl Default for AgentFeedbackOptions {
    fn default() -> Self {
        Self {
            intro: "I'm reviewing the working tree diff. Here are my comments:".into(),
            include_outdated: true,
        }
    }
}

#[must_use]
pub fn format_review(review: &Review, options: &AgentFeedbackOptions) -> String {
    let mut comments: Vec<&ReviewComment> = review
        .comments
        .iter()
        .filter(|comment| options.include_outdated || !comment.outdated)
        .collect();
    comments.sort_by(|left, right| {
        left.anchor
            .path
            .cmp(&right.anchor.path)
            .then_with(|| {
                left.anchor
                    .line_number()
                    .unwrap_or(0)
                    .cmp(&right.anchor.line_number().unwrap_or(0))
            })
            .then_with(|| left.anchor.side.cmp(&right.anchor.side))
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut out = options.intro.clone();
    let mut current: Option<&RepoPath> = None;
    for comment in comments {
        if current != Some(&comment.anchor.path) {
            current = Some(&comment.anchor.path);
            let _ = write!(out, "\n\n## `{}`", comment.anchor.path);
        }
        let side = match comment.anchor.side {
            DiffSide::Old => "removed",
            DiffSide::New => "added",
        };
        let line = comment
            .anchor
            .line_number()
            .map_or_else(|| "unknown".to_owned(), |number| number.to_string());
        let _ = write!(
            out,
            "\n\n**Line {line} ({side}):** `{}`\n> {}",
            comment.context.line_text,
            comment.body.replace('\n', "\n> ")
        );
        if comment.outdated {
            out.push_str("\n> _(outdated)_");
        }
    }
    out
}

/// A repository mutation requested by a diff review UI.
///
/// Renderers emit intents but never execute Git themselves. Embedding hosts
/// execute the action and install a refreshed [`crate::DiffDocument`] snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepositoryAction {
    StagePaths(Vec<RepoPath>),
    UnstagePaths(Vec<RepoPath>),
    StageAll,
    UnstageAll,
    Commit { message: String },
    Discard { path: RepoPath, status: FileStatus },
    Refresh,
}

/// Events emitted by a diff review UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffReviewEvent {
    RepositoryAction(RepositoryAction),
    SubmitReview(ReviewSubmission),
    CopyFormattedReview(String),
    Cancel,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiffDocument, FileDiff};

    fn anchor(file: &FileDiff, side: DiffSide, line: usize) -> LineAnchor {
        LineAnchor::for_line(file, side, 0, line).expect("anchor")
    }

    #[test]
    fn repository_actions_round_trip() {
        let event = DiffReviewEvent::RepositoryAction(RepositoryAction::Discard {
            path: crate::RepoPath::new("src/lib.rs").unwrap(),
            status: crate::FileStatus::Modified,
        });
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(
            serde_json::from_str::<DiffReviewEvent>(&json).unwrap(),
            event
        );
    }

    #[test]
    fn crud_and_round_trip() {
        let file = FileDiff::from_texts("a.rs", "old\n", "new\n").unwrap();
        let mut review = Review::default();
        let id = review.add_comment(anchor(&file, DiffSide::New, 1), "fix");
        assert!(review.edit_comment(id, "fix it"));
        assert_eq!(review.comment(id).unwrap().body, "fix it");
        assert_eq!(review.len(), 1);
        assert!(serde_json::to_string(&review).unwrap().contains("fix it"));
        assert!(review.remove_comment(id).is_some());
        assert!(review.remove_comment(id).is_none());
    }

    #[test]
    fn missing_is_outdated() {
        let file = FileDiff::from_texts("a.rs", "old\n", "new\n").unwrap();
        let mut review = Review::default();
        review.add_comment(anchor(&file, DiffSide::New, 1), "why");
        review.reconcile(&DiffDocument::empty());
        assert!(review.comments()[0].outdated);
        assert_eq!(review.outdated_count(), 1);
    }

    #[test]
    fn moved_content_reattaches_by_nearest_content_fingerprint() {
        let before = FileDiff::from_texts("a.rs", "a\nb\n", "a\ntarget\nb\n").unwrap();
        let mut review = Review::default();
        review.add_comment_with_context(anchor(&before, DiffSide::New, 1), "target", "keep this");

        let after = FileDiff::from_texts("a.rs", "a\nb\n", "a\ninserted\ntarget\nb\n").unwrap();
        review.reconcile(&DiffDocument {
            repo_root: String::new(),
            files: vec![after],
        });

        assert!(!review.comments()[0].outdated);
        assert_eq!(review.comments()[0].anchor.new_line_no, Some(3));
        assert_eq!(review.comments()[0].context.line_text, "target");
    }

    #[test]
    fn an_unchanged_document_restores_exact_anchors() {
        let file = FileDiff::from_texts("a.rs", "a\nb\n", "a\nc\n").unwrap();
        let target = anchor(&file, DiffSide::New, 2);
        let mut review = Review::default();
        review.add_comment(target.clone(), "note");
        review.reconcile(&DiffDocument {
            repo_root: String::new(),
            files: vec![file],
        });
        assert!(!review.comments()[0].outdated);
        assert_eq!(review.comments()[0].anchor, target);
    }

    #[test]
    fn formatting_is_deterministic_and_can_drop_outdated() {
        let file = FileDiff::from_texts("a.rs", "old\n", "new\n").unwrap();
        let mut review = Review::default();
        review.add_comment_with_context(anchor(&file, DiffSide::New, 1), "new", "second\nline");
        review.add_comment_with_context(anchor(&file, DiffSide::Old, 0), "old", "first");
        let formatted = format_review(&review, &AgentFeedbackOptions::default());
        assert!(formatted.find("(removed)").unwrap() < formatted.find("(added)").unwrap());
        assert!(formatted.contains("> second\n> line"));

        review.reconcile(&DiffDocument::empty());
        let hidden = format_review(
            &review,
            &AgentFeedbackOptions {
                include_outdated: false,
                ..AgentFeedbackOptions::default()
            },
        );
        assert!(!hidden.contains("second"));
    }
}
