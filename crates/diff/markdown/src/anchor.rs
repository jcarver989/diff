//! Durable anchors for rendered Markdown targets.
//!
//! Target IDs are intentionally document-local and are not persisted. This module
//! derives a content/context identity from semantic Markdown and resolves that
//! identity against a replacement document.

use crate::{
    Fingerprint, MarkdownBlock, MarkdownBlockKind, MarkdownCodeBlock, MarkdownCodeLine,
    MarkdownDocument, MarkdownLineRange, MarkdownTableRow, MarkdownTarget, MarkdownTargetId,
    MarkdownTargetKind, SourceRange, rendered_text,
};
use serde::{Deserialize, Serialize};

/// Maximum number of Unicode scalar values retained in an anchor snapshot.
pub const SNAPSHOT_CHAR_LIMIT: usize = 512;

/// An identity for either a semantic Markdown block or one code line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkdownAnchor {
    Block(MarkdownBlockAnchor),
    CodeLine(MarkdownCodeLineAnchor),
}

/// Durable identity and context for a non-code-line Markdown target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownBlockAnchor {
    pub source_path: Option<String>,
    pub kind: MarkdownTargetKind,
    pub source_lines: MarkdownLineRange,
    pub content_fingerprint: Fingerprint,
    pub context_fingerprint: Fingerprint,
    pub heading_path: Vec<String>,
    pub snapshot: String,
}

/// Durable identity for a code line, including the identity of its parent block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownCodeLineAnchor {
    pub block: MarkdownBlockAnchor,
    pub code_line_index: usize,
    pub source_line: Option<usize>,
    pub content_fingerprint: Fingerprint,
    pub snapshot: String,
}

impl MarkdownDocument {
    /// Creates a durable anchor for a document-local target.
    #[must_use]
    pub fn anchor_for_target(&self, id: MarkdownTargetId) -> Option<MarkdownAnchor> {
        anchor_for_target(self, id)
    }

    /// Resolves a durable anchor to a target in this document.
    #[must_use]
    pub fn resolve_anchor(&self, anchor: &MarkdownAnchor) -> Option<MarkdownTargetId> {
        resolve_anchor(self, anchor)
    }

    /// Returns the style-free rendered text for a target.
    #[must_use]
    pub fn rendered_target_text(&self, id: MarkdownTargetId) -> Option<String> {
        target_text(self, id)
    }
}

/// Creates an anchor for `id`, retaining a bounded source snapshot.
#[must_use]
pub fn anchor_for_target(
    document: &MarkdownDocument,
    id: MarkdownTargetId,
) -> Option<MarkdownAnchor> {
    let target = document.target(id)?;
    target_details(document, id)?;
    if target.kind == MarkdownTargetKind::CodeLine {
        let (block_id, line) = code_line_parent(document, id)?;
        let block = block_anchor(document, block_id)?;
        return Some(MarkdownAnchor::CodeLine(MarkdownCodeLineAnchor {
            block,
            code_line_index: line.index,
            source_line: line.source_line,
            content_fingerprint: line_content_fingerprint(&line.text),
            snapshot: snapshot(document.source(), &line.source),
        }));
    }
    Some(MarkdownAnchor::Block(block_anchor(document, id)?))
}

/// Resolves an anchor using context, heading path, content, and source proximity.
#[must_use]
pub fn resolve_anchor(
    document: &MarkdownDocument,
    anchor: &MarkdownAnchor,
) -> Option<MarkdownTargetId> {
    match anchor {
        MarkdownAnchor::Block(anchor) => resolve_block(document, anchor),
        MarkdownAnchor::CodeLine(anchor) => {
            let block_id = resolve_block(document, &anchor.block)?;
            let code = code_block(document, block_id)?;
            let matching_index = if code.lines.get(anchor.code_line_index).is_some_and(|line| {
                line_content_fingerprint(&line.text) == anchor.content_fingerprint
            }) {
                Some(anchor.code_line_index)
            } else {
                nearest_line_index(code, anchor)
            }?;
            code.lines
                .get(matching_index)
                .and_then(|line| line.target_id)
        }
    }
}

fn block_anchor(document: &MarkdownDocument, id: MarkdownTargetId) -> Option<MarkdownBlockAnchor> {
    let target = document.target(id)?;
    if target.kind == MarkdownTargetKind::CodeLine {
        return None;
    }
    let text = target_text(document, id)?;
    let normalized = normalize_text(target.kind, &text);
    let target_digest = Fingerprint::of([kind_name(target.kind), normalized.as_str()]);
    let heading_path = heading_path(document, target);
    let previous = neighboring_text(document, id, -1);
    let next = neighboring_text(document, id, 1);
    let surrounding_digest = Fingerprint::of([
        document.source_path().unwrap_or(""),
        kind_name(target.kind),
        normalized.as_str(),
        heading_path.join("\u{1f}").as_str(),
        previous.as_deref().unwrap_or(""),
        next.as_deref().unwrap_or(""),
    ]);
    Some(MarkdownBlockAnchor {
        source_path: document.source_path().map(str::to_owned),
        kind: target.kind,
        source_lines: target.source.lines,
        content_fingerprint: target_digest,
        context_fingerprint: surrounding_digest,
        heading_path,
        snapshot: snapshot(document.source(), &target.source),
    })
}

fn resolve_block(
    document: &MarkdownDocument,
    anchor: &MarkdownBlockAnchor,
) -> Option<MarkdownTargetId> {
    let candidates = document
        .targets()
        .iter()
        .filter(|target| target.kind == anchor.kind)
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }

    let contextual = candidates
        .iter()
        .filter(|target| block_context_fingerprint(document, target) == anchor.context_fingerprint)
        .copied()
        .collect::<Vec<_>>();
    if let Some(id) = nearest_target(&contextual, anchor.source_lines.start) {
        return Some(id);
    }

    let by_heading = candidates
        .iter()
        .filter(|target| {
            target_content_fingerprint(document, target) == anchor.content_fingerprint
                && heading_path(document, target) == anchor.heading_path
        })
        .copied()
        .collect::<Vec<_>>();
    if let Some(id) = nearest_target(&by_heading, anchor.source_lines.start) {
        return Some(id);
    }

    let by_content = candidates
        .iter()
        .filter(|target| target_content_fingerprint(document, target) == anchor.content_fingerprint)
        .copied()
        .collect::<Vec<_>>();
    nearest_target(&by_content, anchor.source_lines.start)
}

fn nearest_target(targets: &[&MarkdownTarget], old_line: usize) -> Option<MarkdownTargetId> {
    let mut nearest = None;
    let mut distance = usize::MAX;
    let mut tied = false;
    for target in targets {
        let candidate_distance = target.source.lines.start.abs_diff(old_line);
        if candidate_distance < distance {
            nearest = Some(target.id);
            distance = candidate_distance;
            tied = false;
        } else if candidate_distance == distance {
            tied = true;
        }
    }
    (!tied).then_some(nearest?).or(None)
}

fn nearest_line_index(
    code: &crate::MarkdownCodeBlock,
    anchor: &MarkdownCodeLineAnchor,
) -> Option<usize> {
    let mut nearest = None;
    let mut distance = usize::MAX;
    let mut tied = false;
    for line in &code.lines {
        if line_content_fingerprint(&line.text) != anchor.content_fingerprint {
            continue;
        }
        let candidate_distance = line.index.abs_diff(anchor.code_line_index);
        if candidate_distance < distance {
            nearest = Some(line.index);
            distance = candidate_distance;
            tied = false;
        } else if candidate_distance == distance {
            tied = true;
        }
    }
    (!tied).then_some(nearest?).or(None)
}

fn block_context_fingerprint(document: &MarkdownDocument, target: &MarkdownTarget) -> Fingerprint {
    let normalized = normalize_text(
        target.kind,
        &target_text(document, target.id).unwrap_or_default(),
    );
    let path = heading_path(document, target);
    Fingerprint::of([
        document.source_path().unwrap_or(""),
        kind_name(target.kind),
        normalized.as_str(),
        path.join("\u{1f}").as_str(),
        neighboring_text(document, target.id, -1)
            .as_deref()
            .unwrap_or(""),
        neighboring_text(document, target.id, 1)
            .as_deref()
            .unwrap_or(""),
    ])
}

fn target_content_fingerprint(document: &MarkdownDocument, target: &MarkdownTarget) -> Fingerprint {
    let normalized = normalize_text(
        target.kind,
        &target_text(document, target.id).unwrap_or_default(),
    );
    Fingerprint::of([kind_name(target.kind), normalized.as_str()])
}

fn line_content_fingerprint(text: &str) -> Fingerprint {
    Fingerprint::of([normalize_code_text(text).as_str()])
}

fn neighboring_text(
    document: &MarkdownDocument,
    id: MarkdownTargetId,
    direction: isize,
) -> Option<String> {
    let index = id.index().checked_add_signed(direction)?;
    let target = document.targets().get(index)?;
    Some(normalize_text(
        target.kind,
        &target_text(document, target.id).unwrap_or_default(),
    ))
}

fn heading_path(document: &MarkdownDocument, target: &MarkdownTarget) -> Vec<String> {
    let mut path = Vec::<(u8, String)>::new();
    for heading in document.outline() {
        if heading.source.lines.start > target.source.lines.start {
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

fn normalize_text(kind: MarkdownTargetKind, text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if matches!(
        kind,
        MarkdownTargetKind::CodeBlock | MarkdownTargetKind::CodeLine
    ) {
        return text
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n");
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_code_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim_end()
        .to_owned()
}

fn snapshot(source: &str, range: &SourceRange) -> String {
    let excerpt = source.get(range.bytes.clone()).unwrap_or("");
    excerpt.chars().take(SNAPSHOT_CHAR_LIMIT).collect()
}

fn kind_name(kind: MarkdownTargetKind) -> &'static str {
    match kind {
        MarkdownTargetKind::Heading => "heading",
        MarkdownTargetKind::Paragraph => "paragraph",
        MarkdownTargetKind::ListItem => "list-item",
        MarkdownTargetKind::BlockQuote => "blockquote",
        MarkdownTargetKind::TableRow => "table-row",
        MarkdownTargetKind::CodeBlock => "code-block",
        MarkdownTargetKind::CodeLine => "code-line",
    }
}

fn target_details(document: &MarkdownDocument, id: MarkdownTargetId) -> Option<String> {
    document
        .blocks()
        .iter()
        .find_map(|block| find_in_block(block, id))
}

fn find_in_block(block: &MarkdownBlock, id: MarkdownTargetId) -> Option<String> {
    if block.target_id == Some(id) {
        return Some(block_text(block));
    }
    match &block.kind {
        MarkdownBlockKind::List { items, .. } => items.iter().find_map(|item| {
            if item.target_id == Some(id) {
                return Some(rendered_text(&item.content));
            }
            item.blocks
                .iter()
                .find_map(|block| find_in_block(block, id))
        }),
        MarkdownBlockKind::BlockQuote { .. }
        | MarkdownBlockKind::Heading { .. }
        | MarkdownBlockKind::Paragraph { .. }
        | MarkdownBlockKind::HtmlFallback { .. }
        | MarkdownBlockKind::Rule => None,
        MarkdownBlockKind::CodeBlock(code) => code
            .lines
            .iter()
            .find_map(|line| (line.target_id == Some(id)).then(|| line.text.clone())),
        MarkdownBlockKind::Table(table) => table
            .rows
            .iter()
            .find_map(|row| (row.target_id == Some(id)).then(|| row_text(row))),
    }
}

fn target_text(document: &MarkdownDocument, id: MarkdownTargetId) -> Option<String> {
    target_details(document, id)
}

fn block_text(block: &MarkdownBlock) -> String {
    match &block.kind {
        MarkdownBlockKind::Heading { content, .. }
        | MarkdownBlockKind::Paragraph { content }
        | MarkdownBlockKind::HtmlFallback { content } => rendered_text(content),
        MarkdownBlockKind::List { items, .. } => items
            .iter()
            .map(|item| rendered_text(&item.content))
            .collect::<Vec<_>>()
            .join(" "),
        MarkdownBlockKind::BlockQuote { blocks } => {
            blocks.iter().map(block_text).collect::<Vec<_>>().join(" ")
        }
        MarkdownBlockKind::CodeBlock(code) => code
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        MarkdownBlockKind::Table(table) => table
            .rows
            .iter()
            .map(row_text)
            .collect::<Vec<_>>()
            .join(" "),
        MarkdownBlockKind::Rule => String::new(),
    }
}

fn row_text(row: &MarkdownTableRow) -> String {
    row.cells
        .iter()
        .map(|cell| rendered_text(&cell.content))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn code_block(document: &MarkdownDocument, id: MarkdownTargetId) -> Option<&MarkdownCodeBlock> {
    document
        .blocks()
        .iter()
        .find_map(|block| code_block_in(block, id))
}

fn code_block_in(block: &MarkdownBlock, id: MarkdownTargetId) -> Option<&crate::MarkdownCodeBlock> {
    match &block.kind {
        MarkdownBlockKind::CodeBlock(code) if code.target_id == Some(id) => Some(code),
        MarkdownBlockKind::List { items, .. } => items.iter().find_map(|item| {
            item.blocks
                .iter()
                .find_map(|block| code_block_in(block, id))
        }),
        _ => None,
    }
}

fn code_line_parent(
    document: &MarkdownDocument,
    id: MarkdownTargetId,
) -> Option<(MarkdownTargetId, &MarkdownCodeLine)> {
    document
        .blocks()
        .iter()
        .find_map(|block| code_line_parent_in(block, id))
}

fn code_line_parent_in(
    block: &MarkdownBlock,
    id: MarkdownTargetId,
) -> Option<(MarkdownTargetId, &MarkdownCodeLine)> {
    match &block.kind {
        MarkdownBlockKind::CodeBlock(code) => code
            .target_id
            .zip(code.lines.iter().find(|line| line.target_id == Some(id))),
        MarkdownBlockKind::List { items, .. } => items.iter().find_map(|item| {
            item.blocks
                .iter()
                .find_map(|block| code_line_parent_in(block, id))
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MarkdownDocument, MarkdownTargetKind};

    fn document(source: &str) -> MarkdownDocument {
        MarkdownDocument::parse_with_metadata(
            Some("plans/plan.md".to_owned()),
            Some("Plan".to_owned()),
            source,
        )
    }

    fn id(document: &MarkdownDocument, kind: MarkdownTargetKind, label: &str) -> MarkdownTargetId {
        document
            .targets()
            .iter()
            .find(|target| target.kind == kind && target.display_label == label)
            .map_or_else(
                || panic!("missing target {kind:?} {label}"),
                |target| target.id,
            )
    }

    #[test]
    fn anchors_round_trip_exactly_and_serialize() {
        let document = document("# Plan\n\nA paragraph.\n\n```rust\nlet value = 1;\n```\n");
        let paragraph = id(&document, MarkdownTargetKind::Paragraph, "A paragraph.");
        let code_line = id(&document, MarkdownTargetKind::CodeLine, "Code line 1");
        let paragraph_anchor = document.anchor_for_target(paragraph).unwrap();
        let line_anchor = document.anchor_for_target(code_line).unwrap();
        assert_eq!(document.resolve_anchor(&paragraph_anchor), Some(paragraph));
        assert_eq!(document.resolve_anchor(&line_anchor), Some(code_line));
        assert!(
            serde_json::to_string(&line_anchor)
                .unwrap()
                .contains("code_line_index")
        );
    }

    #[test]
    fn resolves_moved_blocks_and_changed_source_paths() {
        let old = document("# Plan\n\nKeep this.\n\nOther.\n");
        let target = id(&old, MarkdownTargetKind::Paragraph, "Keep this.");
        let anchor = old.anchor_for_target(target).unwrap();
        let replacement = document("# Plan\n\nInserted.\n\n\nKeep this.\n");
        let replacement_id = replacement
            .targets()
            .iter()
            .find(|candidate| candidate.display_label == "Keep this.")
            .map(|candidate| candidate.id);
        assert_eq!(replacement.resolve_anchor(&anchor), replacement_id);

        let changed_path = MarkdownDocument::parse_with_metadata(
            Some("other.md".to_owned()),
            None,
            "# Plan\n\nKeep this.\n",
        );
        let changed_id = changed_path
            .targets()
            .iter()
            .find(|candidate| candidate.display_label == "Keep this.")
            .map(|candidate| candidate.id);
        assert_eq!(changed_path.resolve_anchor(&anchor), changed_id);
    }

    #[test]
    fn duplicate_content_uses_heading_context_and_rejects_ambiguous_matches() {
        let old = document("# First\n\nSame.\n\n# Second\n\nSame.\n");
        let second = old
            .targets()
            .iter()
            .filter(|target| target.kind == MarkdownTargetKind::Paragraph)
            .nth(1)
            .unwrap();
        let anchor = old.anchor_for_target(second.id).unwrap();
        let replacement = document("# First\n\nSame.\n\n# Second\n\nInserted.\n\nSame.\n");
        let replacement_id = replacement
            .targets()
            .iter()
            .rfind(|candidate| candidate.kind == MarkdownTargetKind::Paragraph)
            .map(|candidate| candidate.id);
        assert_eq!(replacement.resolve_anchor(&anchor), replacement_id);

        let ambiguous = document("Same.\n\nSame.\n");
        let old_without_heading = MarkdownDocument::parse("\nSame.\n");
        let old_anchor = old_without_heading
            .anchor_for_target(old_without_heading.targets()[0].id)
            .unwrap();
        assert_eq!(ambiguous.resolve_anchor(&old_anchor), None);
    }

    #[test]
    fn code_lines_reconcile_after_context_insertions() {
        let old = document("```\none\ntwo\none\n```\n");
        let old_line = old
            .targets()
            .iter()
            .find(|target| {
                target.kind == MarkdownTargetKind::CodeLine && target.display_label == "Code line 2"
            })
            .unwrap();
        let anchor = old.anchor_for_target(old_line.id).unwrap();
        let replacement = document("Inserted context.\n\n```\none\ntwo\none\n```\n");
        let replacement_id = replacement
            .targets()
            .iter()
            .find(|target| {
                target.kind == MarkdownTargetKind::CodeLine && target.display_label == "Code line 2"
            })
            .map(|target| target.id);
        assert_eq!(replacement.resolve_anchor(&anchor), replacement_id);
    }

    #[test]
    fn snapshots_are_bounded_without_splitting_unicode() {
        let source = format!("{}\n", "é".repeat(SNAPSHOT_CHAR_LIMIT + 20));
        let document = MarkdownDocument::parse(&source);
        let anchor = document
            .anchor_for_target(document.targets()[0].id)
            .unwrap();
        let snapshot = match anchor {
            MarkdownAnchor::Block(anchor) => anchor.snapshot,
            MarkdownAnchor::CodeLine(_) => panic!("expected block anchor"),
        };
        assert!(snapshot.chars().count() <= SNAPSHOT_CHAR_LIMIT);
        assert!(snapshot.is_char_boundary(snapshot.len()));
    }
}
