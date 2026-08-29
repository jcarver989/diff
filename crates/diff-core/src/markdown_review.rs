//! Review comments, submissions, and agent-facing feedback for Markdown documents.

use crate::{
    MarkdownAnchor, MarkdownBlock, MarkdownBlockKind, MarkdownDocument, MarkdownLineRange,
    MarkdownTargetId, MarkdownTargetKind, SNAPSHOT_CHAR_LIMIT,
};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, fmt::Write};
use thiserror::Error;

/// The explicit decision made at the end of a Markdown review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkdownReviewDecision {
    Approved,
    ChangesRequested,
}

/// Context retained with a Markdown comment so it remains useful when outdated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownCommentContext {
    pub source_path: Option<String>,
    pub target_label: String,
    pub source_lines: MarkdownLineRange,
    pub source_excerpt: String,
    /// The containing heading path at the time the comment was made.
    pub heading_path: Vec<String>,
    /// The code language, when the target belongs to a fenced code block.
    pub code_language: Option<String>,
}

/// A review comment retained even when its Markdown target disappears.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownReviewComment {
    pub id: u64,
    pub anchor: MarkdownAnchor,
    pub body: String,
    pub context: MarkdownCommentContext,
    pub outdated: bool,
}

/// Errors raised while producing a submission.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MarkdownReviewError {
    #[error("comment {id} has a blank body")]
    BlankComment { id: u64 },
}

/// Mutable collection of Markdown review comments.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownReview {
    comments: Vec<MarkdownReviewComment>,
    next_id: u64,
}

impl MarkdownReview {
    /// Adds a comment with the context available in its anchor.
    pub fn add_comment(&mut self, anchor: MarkdownAnchor, body: impl Into<String>) -> u64 {
        let context = context_from_anchor(&anchor);
        self.add_comment_with_context(anchor, context, body)
    }

    /// Adds a comment with an explicitly captured context.
    pub fn add_comment_with_context(
        &mut self,
        anchor: MarkdownAnchor,
        context: MarkdownCommentContext,
        body: impl Into<String>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.comments.push(MarkdownReviewComment {
            id,
            anchor,
            body: body.into(),
            context,
            outdated: false,
        });
        id
    }

    /// Creates an anchor and captures all context for a document target.
    pub fn add_comment_for_target(
        &mut self,
        document: &MarkdownDocument,
        target: MarkdownTargetId,
        body: impl Into<String>,
    ) -> Option<u64> {
        let anchor = document.anchor_for_target(target)?;
        let context = MarkdownCommentContext::from_target(document, target, &anchor)?;
        Some(self.add_comment_with_context(anchor, context, body))
    }

    /// Changes a comment body, returning whether the ID existed.
    pub fn edit_comment(&mut self, id: u64, body: impl Into<String>) -> bool {
        if let Some(comment) = self.comments.iter_mut().find(|comment| comment.id == id) {
            comment.body = body.into();
            true
        } else {
            false
        }
    }

    /// Removes a comment, returning it if present.
    pub fn remove_comment(&mut self, id: u64) -> Option<MarkdownReviewComment> {
        let index = self.comments.iter().position(|comment| comment.id == id)?;
        Some(self.comments.remove(index))
    }

    #[must_use]
    pub fn comment(&self, id: u64) -> Option<&MarkdownReviewComment> {
        self.comments.iter().find(|comment| comment.id == id)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.comments.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.comments.len()
    }

    #[must_use]
    pub fn outdated_count(&self) -> usize {
        self.comments
            .iter()
            .filter(|comment| comment.outdated)
            .count()
    }

    /// Returns comments in insertion order.
    #[must_use]
    pub fn comments(&self) -> &[MarkdownReviewComment] {
        &self.comments
    }

    /// Iterates over comments in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &MarkdownReviewComment> {
        self.comments.iter()
    }

    /// Returns comments associated with an exact durable anchor.
    #[must_use]
    pub fn comments_for_anchor<'a>(
        &'a self,
        anchor: &MarkdownAnchor,
    ) -> impl DoubleEndedIterator<Item = &'a MarkdownReviewComment> {
        self.comments
            .iter()
            .filter(move |comment| &comment.anchor == anchor)
    }

    /// Returns comments associated with a target in `document`.
    #[must_use]
    pub fn comments_for_target<'a>(
        &'a self,
        document: &'a MarkdownDocument,
        target: MarkdownTargetId,
    ) -> std::vec::IntoIter<&'a MarkdownReviewComment> {
        document.anchor_for_target(target).map_or_else(
            || Vec::new().into_iter(),
            |anchor| {
                self.comments_for_anchor(&anchor)
                    .collect::<Vec<_>>()
                    .into_iter()
            },
        )
    }

    /// Returns the most recently inserted comment ID.
    #[must_use]
    pub fn most_recent_comment_id(&self) -> Option<u64> {
        self.comments.last().map(|comment| comment.id)
    }

    /// Removes every comment without resetting the monotonic ID counter.
    pub fn clear(&mut self) {
        self.comments.clear();
    }

    /// Reattaches comments to a replacement document and marks misses outdated.
    pub fn reconcile(&mut self, document: &MarkdownDocument) {
        for comment in &mut self.comments {
            let Some(target) = document.resolve_anchor(&comment.anchor) else {
                comment.outdated = true;
                continue;
            };
            let Some(anchor) = document.anchor_for_target(target) else {
                comment.outdated = true;
                continue;
            };
            comment.anchor = anchor.clone();
            if let Some(context) = MarkdownCommentContext::from_target(document, target, &anchor) {
                comment.context = context;
            }
            comment.outdated = false;
        }
    }

    /// Produces a submission. Call [`Self::try_submission`] when comments may
    /// have been inserted without a session draft.
    #[must_use]
    pub fn submission(&self, decision: MarkdownReviewDecision) -> MarkdownReviewSubmission {
        self.try_submission(decision)
            .unwrap_or_else(|_| MarkdownReviewSubmission {
                decision,
                comments: self.ordered_comments(),
                formatted: format_markdown_review(self, decision),
            })
    }

    /// Produces a submission after validating that every body is nonblank.
    ///
    /// # Errors
    ///
    /// Returns [`MarkdownReviewError::BlankComment`] when a comment body contains
    /// no non-whitespace characters.
    pub fn try_submission(
        &self,
        decision: MarkdownReviewDecision,
    ) -> Result<MarkdownReviewSubmission, MarkdownReviewError> {
        if let Some(comment) = self
            .comments
            .iter()
            .find(|comment| comment.body.trim().is_empty())
        {
            return Err(MarkdownReviewError::BlankComment { id: comment.id });
        }
        Ok(MarkdownReviewSubmission {
            decision,
            comments: self.ordered_comments(),
            formatted: format_markdown_review(self, decision),
        })
    }

    fn ordered_comments(&self) -> Vec<MarkdownReviewComment> {
        let mut comments = self.comments.clone();
        comments.sort_by(compare_comments);
        comments
    }
}

impl MarkdownCommentContext {
    /// Captures the display and source context for a document target.
    pub fn from_target(
        document: &MarkdownDocument,
        target: MarkdownTargetId,
        anchor: &MarkdownAnchor,
    ) -> Option<Self> {
        let target_info = document.target(target)?;
        let excerpt = match anchor {
            MarkdownAnchor::Block(anchor) => anchor.snapshot.clone(),
            MarkdownAnchor::CodeLine(anchor) => anchor.snapshot.clone(),
        };
        Some(Self {
            source_path: document.source_path().map(str::to_owned),
            target_label: target_info.display_label.clone(),
            source_lines: target_info.source.lines,
            source_excerpt: excerpt.chars().take(SNAPSHOT_CHAR_LIMIT).collect(),
            heading_path: heading_path(document, target_info.source.lines.start),
            code_language: code_language(document, target),
        })
    }
}

fn context_from_anchor(anchor: &MarkdownAnchor) -> MarkdownCommentContext {
    let (kind, source_lines, excerpt, heading_path, code_language) = match anchor {
        MarkdownAnchor::Block(anchor) => (
            anchor.kind,
            anchor.source_lines,
            anchor.snapshot.clone(),
            anchor.heading_path.clone(),
            None,
        ),
        MarkdownAnchor::CodeLine(anchor) => (
            MarkdownTargetKind::CodeLine,
            MarkdownLineRange {
                start: anchor
                    .source_line
                    .unwrap_or(anchor.block.source_lines.start),
                end: anchor
                    .source_line
                    .unwrap_or(anchor.block.source_lines.start),
            },
            anchor.snapshot.clone(),
            anchor.block.heading_path.clone(),
            None,
        ),
    };
    MarkdownCommentContext {
        source_path: match anchor {
            MarkdownAnchor::Block(anchor) => anchor.source_path.clone(),
            MarkdownAnchor::CodeLine(anchor) => anchor.block.source_path.clone(),
        },
        target_label: target_kind_label(kind).to_owned(),
        source_lines,
        source_excerpt: excerpt.chars().take(SNAPSHOT_CHAR_LIMIT).collect(),
        heading_path,
        code_language,
    }
}

fn heading_path(document: &MarkdownDocument, line: usize) -> Vec<String> {
    let mut path: Vec<(u8, String)> = Vec::new();
    for heading in document.outline() {
        if heading.source.lines.start > line {
            break;
        }
        while path
            .last()
            .is_some_and(|(level, _)| *level >= heading.level)
        {
            path.pop();
        }
        path.push((heading.level, heading.title.clone()));
    }
    path.into_iter().map(|(_, title)| title).collect()
}

fn code_language(document: &MarkdownDocument, target: MarkdownTargetId) -> Option<String> {
    document
        .blocks()
        .iter()
        .find_map(|block| code_language_in(block, target))
}

fn code_language_in(block: &MarkdownBlock, target: MarkdownTargetId) -> Option<String> {
    match &block.kind {
        MarkdownBlockKind::CodeBlock(code)
            if code.target_id == Some(target)
                || code.lines.iter().any(|line| line.target_id == Some(target)) =>
        {
            code.language.clone()
        }
        MarkdownBlockKind::List { items, .. } => items.iter().find_map(|item| {
            item.blocks
                .iter()
                .find_map(|block| code_language_in(block, target))
        }),
        MarkdownBlockKind::BlockQuote { blocks } => blocks
            .iter()
            .find_map(|block| code_language_in(block, target)),
        _ => None,
    }
}

/// Formats a Markdown review for an agent or a human copying feedback.
#[must_use]
pub fn format_markdown_review(review: &MarkdownReview, decision: MarkdownReviewDecision) -> String {
    match decision {
        MarkdownReviewDecision::Approved => return "Markdown review approved.".to_owned(),
        MarkdownReviewDecision::ChangesRequested if review.is_empty() => {
            return "Changes requested, but no inline comments were provided.".to_owned();
        }
        MarkdownReviewDecision::ChangesRequested => {}
    }

    let mut comments: Vec<&MarkdownReviewComment> = review.comments.iter().collect();
    comments.sort_by(|left, right| compare_comments(left, right));
    let mut output = String::from("# Markdown review feedback");
    let mut current_path: Option<Option<&str>> = None;
    let mut current_heading: Option<&[String]> = None;

    for comment in comments {
        let path = comment
            .context
            .source_path
            .as_deref()
            .or_else(|| anchor_path(&comment.anchor));
        if current_path != Some(path) {
            let display_path = path.unwrap_or("<document>");
            let _ = write!(output, "\n\n## `{display_path}`");
            current_path = Some(path);
            current_heading = None;
        }
        let heading = comment.context.heading_path.as_slice();
        if heading.is_empty() {
            current_heading = None;
        } else if current_heading != Some(heading) {
            let _ = write!(output, "\n\n### Heading: {}", heading.join(" / "));
            current_heading = Some(heading);
        }

        let _ = write!(output, "\n\n{}", comment_heading(comment));
        if comment.outdated {
            output.push_str(" _(outdated)_");
        }
        let fence = fence_for(&comment.context.source_excerpt);
        let language = comment.context.code_language.as_deref().unwrap_or_else(|| {
            if matches!(&comment.anchor, MarkdownAnchor::Block(anchor) if anchor.kind == MarkdownTargetKind::CodeBlock)
                || matches!(comment.anchor, MarkdownAnchor::CodeLine(_))
            {
                ""
            } else {
                "markdown"
            }
        });
        let _ = write!(
            output,
            "\n\n{fence}{language}\n{}\n{fence}",
            comment.context.source_excerpt
        );
        output.push_str("\n\n");
        quote_body(&mut output, &comment.body);
    }
    output
}

fn compare_comments(left: &MarkdownReviewComment, right: &MarkdownReviewComment) -> Ordering {
    let left_path = left.context.source_path.as_deref().unwrap_or("");
    let right_path = right.context.source_path.as_deref().unwrap_or("");
    left_path
        .cmp(right_path)
        .then_with(|| {
            left.context
                .source_lines
                .start
                .cmp(&right.context.source_lines.start)
        })
        .then_with(|| code_line_index(&left.anchor).cmp(&code_line_index(&right.anchor)))
        .then_with(|| left.id.cmp(&right.id))
        .then_with(|| left.context.heading_path.cmp(&right.context.heading_path))
}

fn code_line_index(anchor: &MarkdownAnchor) -> usize {
    match anchor {
        MarkdownAnchor::CodeLine(anchor) => anchor.code_line_index,
        MarkdownAnchor::Block(_) => usize::MAX,
    }
}

fn comment_heading(comment: &MarkdownReviewComment) -> String {
    let lines = comment.context.source_lines;
    let line_label = if lines.start == lines.end {
        format!("Line {}", lines.start)
    } else {
        format!("Lines {}–{}", lines.start, lines.end)
    };
    let target = match &comment.anchor {
        MarkdownAnchor::CodeLine(anchor) => format!("Code line {}", anchor.code_line_index + 1),
        MarkdownAnchor::Block(anchor) => target_kind_label(anchor.kind).to_owned(),
    };
    format!("### {line_label} — {target}")
}

fn quote_body(output: &mut String, body: &str) {
    for line in body.split('\n') {
        let _ = writeln!(output, "> {line}");
    }
    output.pop();
}

fn fence_for(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for character in text.chars() {
        if character == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    "`".repeat(longest.max(2) + 1)
}

fn anchor_path(anchor: &MarkdownAnchor) -> Option<&str> {
    match anchor {
        MarkdownAnchor::Block(anchor) => anchor.source_path.as_deref(),
        MarkdownAnchor::CodeLine(anchor) => anchor.block.source_path.as_deref(),
    }
}

fn target_kind_label(kind: MarkdownTargetKind) -> &'static str {
    match kind {
        MarkdownTargetKind::Heading => "Heading",
        MarkdownTargetKind::Paragraph => "Paragraph",
        MarkdownTargetKind::ListItem => "List item",
        MarkdownTargetKind::BlockQuote => "Blockquote",
        MarkdownTargetKind::TableRow => "Table row",
        MarkdownTargetKind::CodeBlock => "Code block",
        MarkdownTargetKind::CodeLine => "Code line",
    }
}

/// The durable result emitted by a Markdown review session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownReviewSubmission {
    pub decision: MarkdownReviewDecision,
    pub comments: Vec<MarkdownReviewComment>,
    pub formatted: String,
}

/// Events emitted by a Markdown review frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkdownReviewEvent {
    Submit(MarkdownReviewSubmission),
    CopyFormatted(String),
    Cancel,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MarkdownDocument, MarkdownTargetKind};

    fn document(source: &str) -> MarkdownDocument {
        MarkdownDocument::parse_with_metadata(Some("docs/plan.md".into()), None, source)
    }

    fn target(document: &MarkdownDocument, kind: MarkdownTargetKind) -> MarkdownTargetId {
        document
            .targets()
            .iter()
            .find(|target| target.kind == kind)
            .expect("target")
            .id
    }

    #[test]
    fn captures_context_and_reconciles_outdated_comments() {
        let old = document("# Plan\n\nReview this.\n");
        let id = target(&old, MarkdownTargetKind::Paragraph);
        let mut review = MarkdownReview::default();
        let comment_id = review
            .add_comment_for_target(&old, id, "Please clarify.")
            .unwrap();
        assert_eq!(
            review.comment(comment_id).unwrap().context.target_label,
            "Review this."
        );
        let replacement = document("# Plan\n\nChanged.\n");
        review.reconcile(&replacement);
        assert!(review.comment(comment_id).unwrap().outdated);
        assert_eq!(review.outdated_count(), 1);
    }

    #[test]
    fn formatter_orders_comments_and_quotes_fences() {
        let document = document("# Plan\n\nA `quoted` paragraph.\n");
        let id = target(&document, MarkdownTargetKind::Paragraph);
        let mut review = MarkdownReview::default();
        review
            .add_comment_for_target(&document, id, "first\nsecond")
            .unwrap();
        let output = format_markdown_review(&review, MarkdownReviewDecision::ChangesRequested);
        assert!(output.contains("### Heading: Plan"));
        assert!(output.contains("> first\n> second"));
        assert!(output.contains("```markdown"));
    }

    #[test]
    fn decisions_are_explicit_and_blank_bodies_fail_validation() {
        let document = document("text\n");
        let id = target(&document, MarkdownTargetKind::Paragraph);
        let mut review = MarkdownReview::default();
        review.add_comment_for_target(&document, id, " ");
        assert!(matches!(
            review.try_submission(MarkdownReviewDecision::Approved),
            Err(MarkdownReviewError::BlankComment { .. })
        ));
        assert_eq!(
            format_markdown_review(
                &MarkdownReview::default(),
                MarkdownReviewDecision::ChangesRequested
            ),
            "Changes requested, but no inline comments were provided."
        );
        assert_eq!(
            format_markdown_review(&MarkdownReview::default(), MarkdownReviewDecision::Approved),
            "Markdown review approved."
        );
    }
}
