//! Structured review comments, reconciliation, and agent-facing output.

use crate::{DiffDocument, DiffSide, FileDiff, LineAnchor, PatchLineKind, RepoPath};
use serde::{Deserialize, Serialize};
use std::fmt::Write;

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
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let context = CommentContext {
            path: anchor.path.clone(),
            side: anchor.side,
            line_number: anchor.line_number(),
            line_kind: anchor.kind,
            line_text: String::new(),
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
    /// Adds a comment with explicit captured text.
    pub fn add_comment_with_context(
        &mut self,
        anchor: LineAnchor,
        line_text: impl Into<String>,
        body: impl Into<String>,
    ) -> u64 {
        let id = self.add_comment(anchor, body);
        if let Some(comment) = self.comments.last_mut() {
            comment.context.line_text = line_text.into();
        }
        id
    }
    /// Changes a comment body, returning whether the ID existed.
    pub fn edit_comment(&mut self, id: u64, body: impl Into<String>) -> bool {
        if let Some(c) = self.comments.iter_mut().find(|c| c.id == id) {
            c.body = body.into();
            true
        } else {
            false
        }
    }
    /// Removes a comment, returning it if present.
    pub fn remove_comment(&mut self, id: u64) -> Option<ReviewComment> {
        self.comments
            .iter()
            .position(|c| c.id == id)
            .map(|i| self.comments.remove(i))
    }
    /// Returns whether the review has no comments.
    pub fn is_empty(&self) -> bool {
        self.comments.is_empty()
    }
    /// Returns the number of comments.
    pub fn len(&self) -> usize {
        self.comments.len()
    }
    /// Returns all comments, in insertion order.
    pub fn comments(&self) -> &[ReviewComment] {
        &self.comments
    }
    /// Returns comments associated with a path.
    pub fn comments_for_file(&self, path: &RepoPath) -> impl Iterator<Item = &ReviewComment> {
        self.comments
            .iter()
            .filter(move |c| &c.context.path == path || &c.anchor.path == path)
    }
    /// Removes all comments.
    pub fn clear(&mut self) {
        self.comments.clear();
    }
    /// Reattaches comments to a replacement document and marks misses outdated.
    pub fn reconcile(&mut self, document: &DiffDocument) {
        let candidates: Vec<LineAnchor> = document.files.iter().flat_map(all_anchors).collect();
        for comment in &mut self.comments {
            if let Some(found) = candidates.iter().find(|a| **a == comment.anchor) {
                comment.anchor = found.clone();
                comment.outdated = false;
                continue;
            }
            let old_line = comment.anchor.line_number().unwrap_or(0);
            if let Some(found) = candidates
                .iter()
                .filter(|a| {
                    a.path == comment.anchor.path
                        && a.side == comment.anchor.side
                        && a.content_fingerprint == comment.anchor.content_fingerprint
                })
                .min_by_key(|a| a.line_number().unwrap_or(0).abs_diff(old_line))
            {
                comment.anchor = found.clone();
                comment.outdated = false;
            } else {
                comment.outdated = true;
            }
        }
    }
    /// Creates a deterministic submission using the default formatter.
    pub fn submission(&self) -> ReviewSubmission {
        ReviewSubmission {
            comments: self.comments.clone(),
            formatted: AgentFeedbackFormatter::default().format(self),
        }
    }
}
fn all_anchors(file: &FileDiff) -> impl Iterator<Item = LineAnchor> + '_ {
    file.hunks.iter().enumerate().flat_map(move |(h, hunk)| {
        hunk.lines.iter().enumerate().flat_map(move |(i, line)| {
            let sides: &[DiffSide] = match line.kind {
                PatchLineKind::Removed => &[DiffSide::Old],
                PatchLineKind::Added => &[DiffSide::New],
                _ => &[DiffSide::Old, DiffSide::New],
            };
            sides
                .iter()
                .filter_map(move |side| LineAnchor::for_line(file, *side, h, i))
        })
    })
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
/// Stateless formatter for agent feedback.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentFeedbackFormatter {
    options: AgentFeedbackOptions,
}
impl AgentFeedbackFormatter {
    /// Constructs a formatter with custom options.
    pub fn new(options: AgentFeedbackOptions) -> Self {
        Self { options }
    }
    /// Formats comments with default options.
    pub fn format(&self, review: &Review) -> String {
        self.format_with_options(review, &self.options)
    }
    /// Formats comments with explicit options and stable path/line ordering.
    pub fn format_with_options(&self, review: &Review, options: &AgentFeedbackOptions) -> String {
        let mut comments: Vec<&ReviewComment> = review
            .comments
            .iter()
            .filter(|c| options.include_outdated || !c.outdated)
            .collect();
        comments.sort_by_key(|c| {
            (
                c.anchor.path.clone(),
                c.anchor.line_number().unwrap_or(0),
                c.anchor.side,
                c.id,
            )
        });
        let mut out = options.intro.clone();
        let mut path: Option<&RepoPath> = None;
        for c in comments {
            if path != Some(&c.anchor.path) {
                path = Some(&c.anchor.path);
                let _ = write!(out, "\n\n## `{}`", c.anchor.path);
            }
            let side = match c.anchor.side {
                DiffSide::Old => "removed",
                DiffSide::New => "added",
            };
            let quoted_body = c.body.replace('\n', "\n> ");
            let _ = write!(
                out,
                "\n\n**Line {} ({side}):** `{}`\n> {}",
                c.anchor
                    .line_number()
                    .map_or_else(|| "unknown".into(), |n| n.to_string()),
                c.context.line_text,
                quoted_body
            );
            if c.outdated {
                out.push_str("\n> _(outdated)_");
            }
        }
        out
    }
}
/// Events emitted by a diff review UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffReviewEvent {
    SubmitReview(ReviewSubmission),
    CopyFormattedReview(String),
    Cancel,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileDiff;
    #[test]
    fn crud_and_round_trip() {
        let f = FileDiff::from_texts("a.rs", "old\n", "new\n").unwrap();
        let a = LineAnchor::for_line(&f, DiffSide::New, 0, 1).unwrap();
        let mut r = Review::default();
        let id = r.add_comment(a, "fix");
        assert!(r.edit_comment(id, "fix it"));
        assert_eq!(r.comments().len(), 1);
        assert!(serde_json::to_string(&r).unwrap().contains("fix it"));
        assert!(r.remove_comment(id).is_some());
    }
    #[test]
    fn missing_is_outdated() {
        let f = FileDiff::from_texts("a.rs", "old\n", "new\n").unwrap();
        let a = LineAnchor::for_line(&f, DiffSide::New, 0, 1).unwrap();
        let mut r = Review::default();
        r.add_comment(a, "why");
        let empty = DiffDocument {
            repo_root: String::new(),
            files: vec![],
        };
        r.reconcile(&empty);
        assert!(r.comments()[0].outdated);
    }

    #[test]
    fn moved_content_reattaches_by_nearest_content_fingerprint() {
        let before = FileDiff::from_texts("a.rs", "a\nb\n", "a\ntarget\nb\n").unwrap();
        let anchor = LineAnchor::for_line(&before, DiffSide::New, 0, 1).unwrap();
        let mut review = Review::default();
        review.add_comment_with_context(anchor, "target", "keep this");

        let after = FileDiff::from_texts("a.rs", "a\nb\n", "a\ninserted\ntarget\nb\n").unwrap();
        review.reconcile(&DiffDocument {
            repo_root: String::new(),
            files: vec![after],
        });

        assert!(!review.comments()[0].outdated);
        assert_eq!(review.comments()[0].anchor.new_line_no, Some(3));
        assert_eq!(review.comments()[0].context.line_text, "target");
    }
}
