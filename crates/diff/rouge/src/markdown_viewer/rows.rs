use clankerdiff_markdown::{MarkdownAnchor, MarkdownAnchors, MarkdownDocument, MarkdownTargetId, outline};
use rouge_widgets::{MarkdownRow, MarkdownRows};
use std::sync::Arc;

pub(super) struct Rows {
    pub document: Arc<MarkdownDocument>,
    pub headings: usize,
    markdown: MarkdownRows,
}

impl Rows {
    pub fn new(document: Arc<MarkdownDocument>) -> Self {
        Self { markdown: MarkdownRows::new(document.blocks()), headings: outline(&document).len(), document }
    }

    pub fn len(&self) -> usize {
        self.markdown.len()
    }

    pub fn get(&self, index: usize) -> Option<&MarkdownRow> {
        self.markdown.get(index)
    }

    pub fn index(&self, id: MarkdownTargetId) -> Option<usize> {
        self.markdown.index(id)
    }

    pub fn key(&self, index: usize) -> (Option<MarkdownAnchor>, usize) {
        let anchor = self.markdown.target(index).and_then(|id| self.document.anchor_for_target(id));
        let fallback = if anchor.is_some() { 0 } else { index };
        (anchor, fallback)
    }
}
