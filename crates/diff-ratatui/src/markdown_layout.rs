use crate::text::FitPosition;
use clankerdiff_markdown::{MarkdownTargetId, SourceRange};
use imbl::Vector;
use ratatui::text::Line;
use std::{
    ops::Range,
    sync::{Arc, OnceLock},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MarkdownPresentation {
    #[default]
    Rendered,
    SourceLines,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct MarkdownLayoutOptions {
    pub width: u16,
    pub block_spacing: bool,
    pub presentation: MarkdownPresentation,
    pub wrap: bool,
    pub heading_markers: bool,
    pub preserve_source_gaps: bool,
    pub tab_width: u16,
}
impl Default for MarkdownLayoutOptions {
    fn default() -> Self {
        Self {
            width: 80,
            block_spacing: true,
            presentation: MarkdownPresentation::Rendered,
            wrap: true,
            heading_markers: true,
            preserve_source_gaps: false,
            tab_width: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownRow {
    pub line: Line<'static>,
    pub source: Option<SourceRange>,
    pub target: Option<MarkdownTargetId>,
    pub(crate) checkpoint: Option<RowCheckpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowCheckpoint {
    pub source: SourceRange,
    pub position: FitPosition,
    pub rendered: Arc<str>,
}

pub(crate) type RowChunk = Arc<[Arc<MarkdownRow>]>;
pub(crate) type TargetIndex = imbl::HashMap<MarkdownTargetId, SourceRange>;
#[derive(Debug, Clone, Default)]
pub(crate) struct RowStore {
    rows: Vector<Arc<MarkdownRow>>,
    updates: usize,
}
impl RowStore {
    pub(crate) fn push(&mut self, chunk: &RowChunk) {
        self.updates += chunk.len();
        self.rows.extend(chunk.iter().cloned());
    }

    pub(crate) fn extend(&mut self, rows: &MarkdownRows) {
        self.append(&rows.store, rows.range.clone());
    }

    pub(crate) fn append(&mut self, store: &Self, range: Range<usize>) {
        let mut rows = store.rows.clone();
        rows.truncate(range.end);
        let rows = rows.split_off(range.start);
        self.rows.append(rows);
        self.updates += 1;
    }

    pub(crate) fn truncate(&mut self, len: usize) {
        if len < self.rows.len() {
            self.rows.truncate(len);
            self.updates += 1;
        }
    }

    pub(crate) fn take_updates(&mut self) -> usize {
        std::mem::take(&mut self.updates)
    }

    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }
}

#[derive(Debug, Clone, Default)]
pub struct MarkdownRows {
    store: Arc<RowStore>,
    range: Range<usize>,
}
impl MarkdownRows {
    #[must_use]
    pub fn len(&self) -> usize {
        self.range.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.range.is_empty()
    }
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Arc<MarkdownRow>> {
        if index >= self.len() {
            return None;
        }
        self.store.rows.get(self.range.start + index)
    }
    #[must_use]
    pub fn slice(&self, range: Range<usize>) -> Self {
        assert!(range.start <= range.end && range.end <= self.len());
        Self {
            store: Arc::clone(&self.store),
            range: self.range.start + range.start..self.range.start + range.end,
        }
    }
    #[must_use]
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Arc<MarkdownRow>> {
        self.range.clone().map(move |index| &self.store.rows[index])
    }
}

#[derive(Debug, Clone, Default)]
pub struct MarkdownLayout {
    rows: MarkdownRows,
    flat: Arc<OnceLock<Arc<[Line<'static>]>>>,
    targets: TargetIndex,
}
impl MarkdownLayout {
    pub(crate) fn new(store: RowStore, targets: TargetIndex) -> Self {
        let range = 0..store.len();
        Self {
            rows: MarkdownRows {
                store: Arc::new(store),
                range,
            },
            flat: Arc::default(),
            targets,
        }
    }
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }
    #[must_use]
    pub fn row(&self, index: usize) -> Option<&Arc<MarkdownRow>> {
        self.rows.get(index)
    }
    #[must_use]
    pub const fn rows(&self) -> &MarkdownRows {
        &self.rows
    }
    #[must_use]
    pub fn rows_for_source_line(&self, line: usize) -> Option<Range<usize>> {
        self.matching_rows(|row| {
            row.source
                .as_ref()
                .is_some_and(|source| source.lines.start <= line && source.lines.end >= line)
        })
    }
    #[must_use]
    pub fn rows_for_target(&self, target: MarkdownTargetId) -> Option<Range<usize>> {
        self.matching_rows(|row| row.target == Some(target))
            .or_else(|| {
                let target = self.targets.get(&target)?;
                self.matching_rows(|row| {
                    row.source.as_ref().is_some_and(|source| {
                        source.bytes.start >= target.bytes.start
                            && source.bytes.end <= target.bytes.end
                    })
                })
            })
    }
    fn matching_rows(&self, matches: impl Fn(&MarkdownRow) -> bool) -> Option<Range<usize>> {
        let mut matching = self.rows.iter().enumerate().filter(|(_, row)| matches(row));
        let first = matching.next()?.0;
        let last = matching.last().map_or(first, |(index, _)| index);
        Some(first..last + 1)
    }
    #[must_use]
    pub fn materialize(&self) -> Arc<[Line<'static>]> {
        Arc::clone(
            self.flat
                .get_or_init(|| self.rows.iter().map(|row| row.line.clone()).collect()),
        )
    }
    pub(crate) fn is_materialized(&self) -> bool {
        self.flat.get().is_some()
    }
}

#[derive(Debug, Clone)]
pub struct MarkdownRowUpdate {
    pub base_revision: u64,
    pub revision: u64,
    pub first_changed_row: usize,
    pub replacement: MarkdownRows,
    pub reset: bool,
}
impl MarkdownRowUpdate {
    #[must_use]
    pub fn total_rows(&self) -> usize {
        self.first_changed_row + self.replacement.len()
    }
}
