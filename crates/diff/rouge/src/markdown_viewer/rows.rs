use clankerdiff_markdown::{MarkdownAnchor, MarkdownAnchors, MarkdownDocument, MarkdownTargetId};
use rouge_widgets::{MarkdownRow, MarkdownRows};
use std::sync::Arc;

pub(super) struct Rows {
    pub document: Arc<MarkdownDocument>,
    rows: MarkdownRows,
}

impl Rows {
    pub fn new(document: Arc<MarkdownDocument>) -> Self {
        Self { rows: MarkdownRows::new(document.blocks()), document }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn get(&self, index: usize) -> Option<&MarkdownRow> {
        self.rows.get(index)
    }

    pub fn index(&self, id: MarkdownTargetId) -> Option<usize> {
        self.rows.index(id)
    }

    pub fn key(&self, index: usize) -> (Option<MarkdownAnchor>, usize) {
        let anchor = self.rows.target(index).and_then(|id| self.document.anchor_for_target(id));
        let fallback = if anchor.is_some() { 0 } else { index };
        (anchor, fallback)
    }
}
