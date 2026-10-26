use crate::{
    document::MarkdownDocument,
    incremental::{IncrementalDocument, MarkdownParseStats},
};
use std::{
    ops::Range,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownStreamUpdate {
    pub revision: u64,
    pub changed_source: Range<usize>,
    pub changed_blocks: Range<usize>,
    pub reset: bool,
    pub finished: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkdownStreamChanges {
    pub first_block: usize,
    pub replaced: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MarkdownStreamIdentity(u64);

impl Default for MarkdownStreamIdentity {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Default)]
pub struct MarkdownStream {
    inner: IncrementalDocument,
    revision: u64,
    source_revision: u64,
    identity: MarkdownStreamIdentity,
    finished: bool,
    changes: Vec<(u64, usize)>,
    replaced_at: u64,
}

impl Clone for MarkdownStream {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            revision: self.revision,
            source_revision: self.source_revision,
            identity: MarkdownStreamIdentity::default(),
            finished: self.finished,
            changes: self.changes.clone(),
            replaced_at: self.replaced_at,
        }
    }
}

impl MarkdownStream {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: &str) -> MarkdownStreamUpdate {
        if chunk.is_empty() && !self.finished {
            return self.unchanged();
        }
        let start = self.source().len();
        let reset = std::mem::take(&mut self.finished);
        let mut first_block = self.inner.document().blocks().len();
        if !chunk.is_empty() {
            first_block = self.inner.append(chunk);
            self.source_revision = self.source_revision.wrapping_add(1);
        }
        self.revision = self.revision.wrapping_add(1);
        self.record(first_block);
        self.update(start..self.source().len(), first_block, reset)
    }

    pub fn replace(&mut self, source: impl Into<String>) -> MarkdownStreamUpdate {
        let source = source.into();
        self.inner.replace(&source);
        self.source_revision = self.source_revision.wrapping_add(1);
        self.finished = false;
        self.revision = self.revision.wrapping_add(1);
        self.changes.clear();
        self.replaced_at = self.revision;
        self.update(0..self.source().len(), 0, true)
    }

    pub fn finish(&mut self) -> MarkdownStreamUpdate {
        if self.finished {
            return self.unchanged();
        }
        self.finished = true;
        self.revision = self.revision.wrapping_add(1);
        let len = self.source().len();
        self.update(len..len, self.block_count(), false)
    }

    #[must_use]
    pub fn source(&self) -> &str {
        self.inner.document().source()
    }

    #[must_use]
    pub const fn document(&self) -> &MarkdownDocument {
        self.inner.document()
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn source_revision(&self) -> u64 {
        self.source_revision
    }

    #[must_use]
    pub const fn identity(&self) -> MarkdownStreamIdentity {
        self.identity
    }

    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    #[must_use]
    pub const fn settled_blocks(&self) -> usize {
        self.inner.settled_blocks()
    }

    #[must_use]
    pub fn open_code_block(&self) -> Option<usize> {
        self.inner.open_code_block()
    }

    #[must_use]
    pub const fn parse_stats(&self) -> MarkdownParseStats {
        self.inner.stats()
    }

    #[must_use]
    pub fn changes_since(&self, revision: u64) -> MarkdownStreamChanges {
        if revision < self.replaced_at {
            return MarkdownStreamChanges {
                first_block: 0,
                replaced: true,
            };
        }
        let position = self
            .changes
            .partition_point(|(changed_at, _)| *changed_at <= revision);
        MarkdownStreamChanges {
            first_block: self
                .changes
                .get(position)
                .map_or_else(|| self.block_count(), |(_, block)| *block),
            replaced: false,
        }
    }

    fn block_count(&self) -> usize {
        self.inner.document().blocks().len()
    }

    fn record(&mut self, first_block: usize) {
        if first_block >= self.block_count() {
            return;
        }
        while self
            .changes
            .last()
            .is_some_and(|(_, block)| *block >= first_block)
        {
            self.changes.pop();
        }
        self.changes.push((self.revision, first_block));
    }

    fn update(
        &self,
        changed_source: Range<usize>,
        first_block: usize,
        reset: bool,
    ) -> MarkdownStreamUpdate {
        MarkdownStreamUpdate {
            revision: self.revision,
            changed_source,
            changed_blocks: first_block.min(self.block_count())..self.block_count(),
            reset,
            finished: self.finished,
        }
    }

    fn unchanged(&self) -> MarkdownStreamUpdate {
        let len = self.source().len();
        self.update(len..len, self.block_count(), false)
    }
}
