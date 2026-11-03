//! Read-only whole-document and append-stream Markdown rendering.

use crate::{
    color::{layered_style, page_color},
    markdown_layout::{
        MarkdownLayout, MarkdownLayoutOptions, MarkdownPresentation, MarkdownRow, MarkdownRowUpdate, RowCheckpoint,
        RowChunk, RowStore, TargetIndex,
    },
    syntax::highlighted_line,
    text::{FitOptions, FitPosition, fit_spans_from},
};
pub use clankerdiff_markdown::{
    Fingerprint, FingerprintError, MarkdownAnchor, MarkdownBlock, MarkdownBlockAnchor, MarkdownBlockKind,
    MarkdownCodeBlock, MarkdownCodeLine, MarkdownCodeLineAnchor, MarkdownCommentContext, MarkdownCommentDraft,
    MarkdownDocument, MarkdownFocusPane, MarkdownHeading, MarkdownInline, MarkdownLineRange, MarkdownListItem,
    MarkdownOptions, MarkdownParseStats, MarkdownReview, MarkdownReviewCommand, MarkdownReviewComment,
    MarkdownReviewDecision, MarkdownReviewError, MarkdownReviewEvent, MarkdownReviewSession, MarkdownReviewSubmission,
    MarkdownSourceRole, MarkdownSourceStyle, MarkdownStream, MarkdownStreamChanges, MarkdownStreamIdentity,
    MarkdownStreamUpdate, MarkdownTable, MarkdownTableAlignment, MarkdownTableCell, MarkdownTableRow, MarkdownTarget,
    MarkdownTargetId, MarkdownTargetKind, SNAPSHOT_CHAR_LIMIT, SourceRange, anchor_for_target, format_markdown_review,
    parse_markdown, rendered_text, resolve_anchor,
};
use clankerdiff_syntax::{HighlightSpan, SyntaxTheme, thread_work};
use clankerdiff_ui_core::{ReviewTheme, Rgba};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use similar::{ChangeTag, TextDiff};
use std::{collections::HashMap, ops::Range, sync::Arc};
use thiserror::Error;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Deterministic work counters for incremental Markdown rendering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarkdownRenderStats {
    /// Bytes actually supplied to the Markdown parser.
    pub parsed_bytes: usize,
    pub scanned_bytes: usize,
    pub source_bytes_copied: usize,
    pub prefix_bytes_copied: usize,
    pub parsed_documents: u64,
    pub rows_generated: usize,
    pub rows_reused: usize,
    pub rows_compared: usize,
    pub rows_materialized: usize,
    pub highlighted_bytes: usize,
    pub blocks_visited: usize,
    pub targets_visited: usize,
    pub chunks_visited: usize,
    pub row_store_updates: usize,
}

impl MarkdownRenderStats {
    fn record_parse(&mut self, parse: MarkdownParseStats, seen: MarkdownParseStats) {
        self.parsed_bytes += parse.parsed_bytes.saturating_sub(seen.parsed_bytes);
        self.scanned_bytes += parse.scanned_bytes.saturating_sub(seen.scanned_bytes);
        self.source_bytes_copied += parse.source_bytes_copied.saturating_sub(seen.source_bytes_copied);
        self.prefix_bytes_copied += parse.prefix_bytes_copied.saturating_sub(seen.prefix_bytes_copied);
        self.parsed_documents += parse.parses.saturating_sub(seen.parses);
        self.highlighted_bytes += parse.highlighted_bytes.saturating_sub(seen.highlighted_bytes);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StreamingMarkdownPolicy {
    #[default]
    Reflowable,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MarkdownCommitError {
    #[error("row commits require the terminal streaming policy")]
    Reflowable,
    #[error("commit base revision {base} does not match the layout revision {revision}")]
    StaleRevision { base: u64, revision: u64 },
    #[error("cannot commit {requested} rows when {committed} rows are already committed")]
    Decreasing { committed: usize, requested: usize },
    #[error("cannot commit {requested} rows of a layout with {rows} rows")]
    OutOfRange { rows: usize, requested: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MarkdownStreamError {
    #[error("{committed} committed rows cannot be rewritten by a replaced or different stream")]
    CommittedSourceReplaced { committed: usize },
}

#[derive(Debug, Clone, Default)]
pub struct StreamingMarkdownState {
    policy: StreamingMarkdownPolicy,
    layout: MarkdownLayout,
    incremental: IncrementalLayout,
    history: History,
    revision: u64,
    /// Last revision handed out; survives `reset` so hosts never see a reuse.
    next_revision: u64,
    update: Option<MarkdownRowUpdate>,
    stats: MarkdownRenderStats,
}

impl StreamingMarkdownState {
    #[must_use]
    pub fn new(policy: StreamingMarkdownPolicy) -> Self {
        Self { policy, ..Self::default() }
    }

    #[must_use]
    pub const fn policy(&self) -> StreamingMarkdownPolicy {
        self.policy
    }

    pub fn reset(&mut self) {
        *self = Self {
            policy: self.policy,
            history: std::mem::take(&mut self.history),
            next_revision: self.next_revision,
            ..Self::default()
        };
    }

    /// Returns accumulated renderer work counters and resets them.
    pub fn take_stats(&mut self) -> MarkdownRenderStats {
        std::mem::take(&mut self.stats)
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn committed_rows(&self) -> usize {
        self.history.rows
    }

    #[must_use]
    pub fn update_since(&self, base_revision: u64) -> MarkdownRowUpdate {
        let rows = self.layout.rows();
        if base_revision == self.revision {
            return MarkdownRowUpdate {
                base_revision,
                revision: self.revision,
                first_changed_row: rows.len(),
                replacement: rows.slice(rows.len()..rows.len()),
                reset: false,
            };
        }
        if let Some(update) = &self.update
            && update.base_revision == base_revision
        {
            return update.clone();
        }
        let committed = self.history.rows.min(rows.len());
        MarkdownRowUpdate {
            base_revision,
            revision: self.revision,
            first_changed_row: committed,
            replacement: rows.slice(committed..rows.len()),
            reset: committed == 0,
        }
    }

    pub fn commit_rows(&mut self, base_revision: u64, end_exclusive: usize) -> Result<(), MarkdownCommitError> {
        if self.policy != StreamingMarkdownPolicy::Terminal {
            return Err(MarkdownCommitError::Reflowable);
        }
        if base_revision != self.revision {
            return Err(MarkdownCommitError::StaleRevision { base: base_revision, revision: self.revision });
        }
        let committed = self.history.rows;
        if end_exclusive < committed {
            return Err(MarkdownCommitError::Decreasing { committed, requested: end_exclusive });
        }
        let rows = self.layout.row_count();
        if end_exclusive > rows {
            return Err(MarkdownCommitError::OutOfRange { rows, requested: end_exclusive });
        }
        if end_exclusive == committed {
            return Ok(());
        }
        let mut frozen = RowStore::default();
        frozen.extend(&self.layout.rows().slice(0..end_exclusive));
        self.stats.row_store_updates += frozen.take_updates();
        self.history.store = frozen;
        self.history.rows = end_exclusive;
        self.update = None;
        if let Some(boundary) = self.incremental.boundary_at(end_exclusive) {
            let mut boundary = boundary;
            boundary.checkpoint = self.layout.row(end_exclusive - 1).and_then(|row| row.checkpoint.clone());
            self.history.boundary = Some(boundary);
        }
        if self.history.bound.is_none() {
            self.history.bound = self.incremental.identity.map(|identity| (identity, self.incremental.revision));
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Renders a canonical semantic document without review gutters or controls.
    #[must_use]
    pub fn render_lines(
        &self,
        document: &MarkdownDocument,
        options: MarkdownLayoutOptions,
        theme: &ReviewTheme,
    ) -> Arc<[Line<'static>]> {
        self.render_layout(document, options, theme).materialize()
    }

    pub fn render_stream_lines(
        &self,
        state: &mut StreamingMarkdownState,
        stream: &MarkdownStream,
        options: MarkdownLayoutOptions,
        theme: &ReviewTheme,
    ) -> Result<Arc<[Line<'static>]>, MarkdownStreamError> {
        let layout = self.render_stream_layout(state, stream, options, theme)?;
        if !layout.is_materialized() {
            state.stats.rows_materialized += layout.row_count();
        }
        Ok(layout.materialize())
    }

    #[must_use]
    pub fn render_layout(
        &self,
        document: &MarkdownDocument,
        options: MarkdownLayoutOptions,
        theme: &ReviewTheme,
    ) -> MarkdownLayout {
        let mut incremental = IncrementalLayout::default();
        let mut stats = MarkdownRenderStats::default();
        let build = incremental.build(
            document,
            None,
            (0, 0),
            true,
            options,
            theme,
            theme.revision(),
            &History::default(),
            &mut stats,
        );
        MarkdownLayout::new(build.store, incremental.targets)
    }

    pub fn render_stream_layout(
        &self,
        state: &mut StreamingMarkdownState,
        stream: &MarkdownStream,
        options: MarkdownLayoutOptions,
        theme: &ReviewTheme,
    ) -> Result<MarkdownLayout, MarkdownStreamError> {
        let theme_revision = theme.revision();
        let document = stream.document();
        state.history.validate_source(stream)?;
        let same_stream = state.incremental.identity == Some(stream.identity());
        let changes = same_stream.then(|| stream.changes_since(state.incremental.revision));
        let same_shape = state.incremental.options == Some(options) && state.incremental.theme == Some(theme_revision);
        let same_source = same_stream && state.incremental.source_revision == stream.source_revision();
        let parse = stream.parse_stats();
        let seen = if same_stream { state.incremental.parse_stats } else { MarkdownParseStats::default() };
        state.stats.record_parse(parse, seen);
        state.incremental.parse_stats = parse;
        let unchanged = same_shape
            && same_source
            && changes.is_some_and(|changes| !changes.replaced && changes.first_block >= document.blocks().len());
        if unchanged {
            state.incremental.revision = stream.revision();
            state.stats.rows_reused += state.layout.row_count();
            return Ok(state.layout.clone());
        }
        let (first, reset) = match changes {
            Some(changes) if same_shape && !changes.replaced => ((changes.first_block, changes.first_line), false),
            _ => ((0, 0), true),
        };
        let highlighted_before = thread_work().parser_input_bytes;
        let build = state.incremental.build(
            document,
            stream.open_code_block(),
            first,
            reset,
            options,
            theme,
            theme_revision,
            &state.history,
            &mut state.stats,
        );
        state.stats.highlighted_bytes += thread_work().parser_input_bytes - highlighted_before;
        state.incremental.identity = Some(stream.identity());
        state.incremental.revision = stream.revision();
        state.incremental.source_revision = stream.source_revision();
        state.incremental.options = Some(options);
        state.incremental.theme = Some(theme_revision);
        let layout = MarkdownLayout::new(build.store, state.incremental.targets.clone());
        let previous = state.layout.rows();
        let reset = reset && state.history.rows == 0;
        let mut first_changed_row =
            if reset { 0 } else { build.first_generated.unwrap_or(layout.row_count()).min(previous.len()) };
        while !reset
            && first_changed_row < layout.row_count()
            && let (Some(left), Some(right)) = (previous.get(first_changed_row), layout.row(first_changed_row))
        {
            state.stats.rows_compared += 1;
            if Arc::ptr_eq(left, right) || left == right {
                first_changed_row += 1;
            } else {
                break;
            }
        }
        if first_changed_row == layout.row_count() && layout.row_count() == previous.len() {
            return Ok(state.layout.clone());
        }
        let mut store = RowStore::default();
        store.extend(&previous.slice(0..first_changed_row));
        store.extend(&layout.rows().slice(first_changed_row..layout.row_count()));
        state.stats.row_store_updates += store.take_updates();
        state.incremental.store = store.clone();
        let layout = MarkdownLayout::new(store, state.incremental.targets.clone());
        state.next_revision += 1;
        let revision = state.next_revision;
        state.update = Some(MarkdownRowUpdate {
            base_revision: state.revision,
            revision,
            first_changed_row,
            replacement: layout.rows().slice(first_changed_row..layout.row_count()),
            reset,
        });
        state.revision = revision;
        state.layout = layout;
        Ok(state.layout.clone())
    }
}

#[derive(Debug, Clone, Default)]
struct History {
    store: RowStore,
    rows: usize,
    boundary: Option<Boundary>,
    bound: Option<(MarkdownStreamIdentity, u64)>,
}

impl History {
    fn validate_source(&self, stream: &MarkdownStream) -> Result<(), MarkdownStreamError> {
        if let Some((identity, revision)) = self.bound
            && (stream.identity() != identity || stream.changes_since(revision).replaced)
        {
            return Err(MarkdownStreamError::CommittedSourceReplaced { committed: self.rows });
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct Boundary {
    block_start: usize,
    rows: usize,
    options: MarkdownLayoutOptions,
    checkpoint: Option<RowCheckpoint>,
}

struct LayoutBuild {
    store: RowStore,
    first_generated: Option<usize>,
}

#[derive(Debug, Clone, Default)]
struct Unit {
    block_start: usize,
    store: RowStore,
    first_generated: Option<usize>,
    len: usize,
}

impl Unit {
    fn push(&mut self, chunk: &RowChunk, generated: bool) {
        if generated && !chunk.is_empty() && self.first_generated.is_none() {
            self.first_generated = Some(self.len);
        }
        self.len += chunk.len();
        self.store.push(chunk);
    }
}

#[derive(Debug, Clone, Default)]
struct OpenCode {
    block: usize,
    checkpoint: Option<RowCheckpoint>,
    options: Option<MarkdownLayoutOptions>,
    store: RowStore,
    first_line: usize,
    line_ends: Vec<usize>,
}

#[derive(Debug, Clone, Default)]
struct IncrementalLayout {
    identity: Option<MarkdownStreamIdentity>,
    revision: u64,
    source_revision: u64,
    options: Option<MarkdownLayoutOptions>,
    theme: Option<Fingerprint>,
    parse_stats: MarkdownParseStats,
    store: RowStore,
    units: Vec<Unit>,
    block_ends: Vec<usize>,
    skipped_rows: Vec<usize>,
    open: Option<OpenCode>,
    spacer: Option<RowChunk>,
    targets: TargetIndex,
    target_ids: Vec<MarkdownTargetId>,
    source_lines: Vec<RowChunk>,
    line_ranges: Vec<SourceRange>,
}

impl IncrementalLayout {
    fn boundary_at(&self, offset: usize) -> Option<Boundary> {
        let options = self.options?;
        let index = self.block_ends.partition_point(|end| *end <= offset);
        let index = index.min(self.units.len().checked_sub(1)?);
        let unit = self.units.get(index)?;
        let start = index.checked_sub(1).map_or(0, |previous| self.block_ends[previous]);
        Some(Boundary {
            block_start: unit.block_start,
            rows: self.skipped_rows[index] + offset.saturating_sub(start),
            options,
            checkpoint: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        &mut self,
        document: &MarkdownDocument,
        open_block: Option<usize>,
        first: (usize, usize),
        reset: bool,
        options: MarkdownLayoutOptions,
        theme: &ReviewTheme,
        theme_revision: Fingerprint,
        history: &History,
        stats: &mut MarkdownRenderStats,
    ) -> LayoutBuild {
        if self.options != Some(options) || self.theme != Some(theme_revision) {
            self.spacer = None;
        }
        if reset {
            if let Some(open) = &mut self.open {
                open.options = None;
            }
            self.source_lines.clear();
            self.line_ranges.clear();
        }
        self.open.take_if(|open| Some(open.block) != open_block);
        self.sync_targets(document, first, open_block, stats);
        let mut store = history.store.clone();
        store.take_updates();
        stats.rows_reused += history.rows;
        let mut first_generated = None;
        match options.presentation {
            MarkdownPresentation::Rendered => {
                self.build_rendered(
                    document,
                    open_block,
                    first,
                    options,
                    theme,
                    history,
                    stats,
                    &mut store,
                    &mut first_generated,
                );
            }
            MarkdownPresentation::SourceLines => {
                self.units.clear();
                self.block_ends.clear();
                self.skipped_rows.clear();
                self.build_source_lines(
                    document,
                    open_block,
                    first,
                    options,
                    theme,
                    history,
                    stats,
                    &mut store,
                    &mut first_generated,
                );
            }
        }
        stats.row_store_updates += store.take_updates();
        self.store = store.clone();
        LayoutBuild { store, first_generated }
    }

    fn sync_targets(
        &mut self,
        document: &MarkdownDocument,
        (first_block, first_line): (usize, usize),
        open_block: Option<usize>,
        stats: &mut MarkdownRenderStats,
    ) {
        let targets = document.targets();
        let mut first_target = document.blocks().get(first_block).map_or(targets.len(), |block| {
            targets.partition_point(|target| target.source.bytes.start < block.source.bytes.start)
        });
        let code_prefix = match document.blocks().get(first_block).map(|block| &block.kind) {
            Some(MarkdownBlockKind::CodeBlock(code)) if open_block == Some(first_block) => {
                code.target_id.map(|root| (root.index(), root.index() + 1 + first_line.min(code.lines().len())))
            }
            _ => None,
        };
        if let Some((root, first_line)) = code_prefix {
            if let Some(target) = targets.get(root) {
                stats.targets_visited += 1;
                self.targets.insert(target.id, target.source.clone());
            }
            first_target = first_line.min(targets.len());
        } else if first_block == 0 {
            self.targets.clear();
            self.target_ids.clear();
        }
        let first_target = first_target.min(self.target_ids.len());
        for id in self.target_ids.drain(first_target..) {
            stats.targets_visited += 1;
            self.targets.remove(&id);
        }
        for target in &targets[first_target..] {
            stats.targets_visited += 1;
            self.targets.insert(target.id, target.source.clone());
            self.target_ids.push(target.id);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_rendered(
        &mut self,
        document: &MarkdownDocument,
        open_block: Option<usize>,
        (first_block, first_line): (usize, usize),
        options: MarkdownLayoutOptions,
        theme: &ReviewTheme,
        history: &History,
        stats: &mut MarkdownRenderStats,
        store: &mut RowStore,
        first_generated: &mut Option<usize>,
    ) {
        let changed_block = first_block;
        let blocks = document.blocks();
        let boundary_index = history.boundary.as_ref().map_or(0, |boundary| {
            blocks.partition_point(|block| {
                boundary.checkpoint.as_ref().map_or(block.source.bytes.start < boundary.block_start, |checkpoint| {
                    block.source.bytes.end <= checkpoint.source.bytes.start
                })
            })
        });
        let first_block = first_block.max(boundary_index).min(blocks.len());
        let prefix = first_block
            .checked_sub(1)
            .and_then(|index| self.block_ends.get(index).copied())
            .unwrap_or(history.rows)
            .max(history.rows);
        if first_block > boundary_index && prefix <= self.store.len() {
            *store = self.store.clone();
            store.take_updates();
            store.truncate(prefix);
            stats.rows_reused += prefix.saturating_sub(history.rows);
        }
        self.units.truncate(first_block);
        self.units.resize_with(first_block, Unit::default);
        self.block_ends.truncate(first_block);
        self.block_ends.resize(first_block, history.rows);
        self.skipped_rows.truncate(first_block);
        self.skipped_rows.resize(first_block, 0);
        let mut next_source_line =
            first_block.checked_sub(1).map_or(1, |index| blocks[index].source.lines.end.saturating_add(1));
        for (index, block) in blocks.iter().enumerate().skip(first_block) {
            stats.blocks_visited += 1;
            let start = block.source.bytes.start;
            let gap = next_source_line..block.source.lines.start;
            next_source_line = block.source.lines.end.saturating_add(1);
            let boundary = history.boundary.as_ref();
            let at_boundary = boundary.filter(|boundary| {
                boundary.checkpoint.as_ref().map_or(boundary.block_start == start, |checkpoint| {
                    start <= checkpoint.source.bytes.start && checkpoint.source.bytes.start < block.source.bytes.end
                })
            });
            let checkpoint = at_boundary.and_then(|boundary| boundary.checkpoint.as_ref());
            let unit_options =
                at_boundary.filter(|_| checkpoint.is_none()).map_or(options, |boundary| boundary.options);
            let changed_line = if index == changed_block { first_line } else { 0 };
            let open = (open_block == Some(index)).then_some(changed_line);
            let unit = self.render_unit(document, index, block, gap, open, unit_options, checkpoint, theme, stats);
            self.units.push(unit.clone());
            let skip = at_boundary.filter(|_| checkpoint.is_none()).map_or(0, |boundary| boundary.rows);
            self.skipped_rows.push(skip.min(unit.len));
            emit_unit(&unit, skip, store, first_generated);
            self.block_ends.push(store.len());
        }
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn render_unit(
        &mut self,
        document: &MarkdownDocument,
        index: usize,
        block: &MarkdownBlock,
        gap: Range<usize>,
        open_changed_line: Option<usize>,
        options: MarkdownLayoutOptions,
        checkpoint: Option<&RowCheckpoint>,
        theme: &ReviewTheme,
        stats: &mut MarkdownRenderStats,
    ) -> Unit {
        let mut unit = Unit { block_start: block.source.bytes.start, ..Unit::default() };
        if options.preserve_source_gaps {
            if checkpoint.is_none() && !gap.is_empty() {
                self.line_ranges(document.source());
                let ranges = &self.line_ranges;
                let rows = gap
                    .clone()
                    .map(|line| {
                        Arc::new(MarkdownRow {
                            line: Line::default(),
                            source: ranges.get(line - 1).cloned(),
                            target: None,
                            checkpoint: None,
                        })
                    })
                    .collect::<RowChunk>();
                stats.rows_generated += rows.len();
                unit.push(&rows, true);
            }
        } else if checkpoint.is_none() && index > 0 && options.block_spacing {
            let spacer = self.spacer.get_or_insert_with(|| {
                Arc::from([Arc::new(MarkdownRow {
                    line: Line::default(),
                    source: None,
                    target: None,
                    checkpoint: None,
                })])
            });
            unit.push(spacer, true);
        }
        let context = BlockContext { target: None, foreground: theme.diff.foreground, prefix: "" };
        if let (MarkdownBlockKind::CodeBlock(code), Some(changed_from)) = (&block.kind, open_changed_line) {
            let highlights = &code.syntax().highlights;
            let open = match &mut self.open {
                Some(open) if open.block == index => open,
                slot => slot.insert(OpenCode { block: index, ..OpenCode::default() }),
            };
            let first_line = checkpoint.map_or(0, |checkpoint| {
                code.lines().partition_point(|line| line.source.bytes.end <= checkpoint.source.bytes.start)
            });
            let reuse = if open.options == Some(options)
                && open.checkpoint.as_ref() == checkpoint
                && open.first_line == first_line
            {
                changed_from.saturating_sub(first_line).min(open.line_ends.len())
            } else {
                0
            };
            open.options = Some(options);
            open.checkpoint = checkpoint.cloned();
            let retained = reuse.checked_sub(1).map_or(0, |index| open.line_ends[index]);
            open.store.truncate(retained);
            open.line_ends.truncate(reuse);
            open.first_line = first_line;
            stats.rows_reused += retained;
            if reuse > 0 {
                unit.first_generated = None;
            }
            let base = fenced_code_style(theme);
            for (line_index, line) in code.lines().iter().enumerate().skip(first_line + reuse) {
                stats.chunks_visited += 1;
                let mut output = RowOutput::new(options).after(checkpoint);
                render_code_line(
                    line,
                    highlights.line(line_index).unwrap_or_default(),
                    &theme.syntax,
                    base,
                    "",
                    line.target_id.or(code.target_id),
                    &mut output,
                );
                let chunk: RowChunk = Arc::from(output.rows);
                stats.rows_generated += chunk.len();
                open.store.push(&chunk);
                open.line_ends.push(open.store.len());
            }
            if unit.first_generated.is_none() && retained < open.store.len() {
                unit.first_generated = Some(unit.len + retained);
            }
            unit.store.append(&open.store, 0..open.store.len());
            unit.len += open.store.len();
            stats.row_store_updates += open.store.take_updates() + unit.store.take_updates();
            return unit;
        }
        let mut output = RowOutput::new(options).after(checkpoint);
        render_block(block, theme, &mut output, context);
        let chunk: RowChunk = Arc::from(output.rows);
        stats.rows_generated += chunk.len();
        stats.chunks_visited += 1;
        unit.push(&chunk, true);
        stats.row_store_updates += unit.store.take_updates();
        unit
    }

    fn line_ranges(&mut self, source: &str) -> usize {
        let previous = self.line_ranges.len();
        let mut cursor = match self.line_ranges.pop() {
            Some(last) if last.bytes.start <= source.len() => last.bytes.start,
            _ => {
                self.line_ranges.clear();
                0
            }
        };
        let mut line = self.line_ranges.len() + 1;
        loop {
            let end = source[cursor..].find('\n').map(|offset| cursor + offset + 1);
            self.line_ranges.push(SourceRange {
                bytes: cursor..end.unwrap_or(source.len()),
                lines: MarkdownLineRange { start: line, end: line },
            });
            line += 1;
            match end {
                Some(end) => cursor = end,
                None => break,
            }
        }
        previous.saturating_sub(1)
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn build_source_lines(
        &mut self,
        document: &MarkdownDocument,
        open_block: Option<usize>,
        (first_block, first_code_line): (usize, usize),
        options: MarkdownLayoutOptions,
        theme: &ReviewTheme,
        history: &History,
        stats: &mut MarkdownRenderStats,
        store: &mut RowStore,
        first_generated: &mut Option<usize>,
    ) {
        let source = document.source();
        let blocks = document.blocks();
        let first_block = if history.boundary.as_ref().is_some_and(|boundary| boundary.checkpoint.is_some()) {
            0
        } else {
            first_block
        };
        let first_byte = blocks.get(first_block).map_or(source.len(), |block| block.source.bytes.start);
        let mut first_line =
            blocks.get(first_block).map_or(usize::MAX, |block| block.source.lines.start.saturating_sub(1));
        first_line = first_line.min(self.line_ranges(source));
        let code_style = fenced_code_style(theme);
        let mut code_lines = HashMap::new();
        for (index, block) in blocks.iter().enumerate().skip(first_block) {
            stats.blocks_visited += 1;
            for code in block.code_blocks() {
                let highlights = &code.syntax().highlights;
                if open_block == Some(index) && index == first_block {
                    let content_line = code.content.lines.start.saturating_sub(1);
                    first_line = first_line.max(content_line + first_code_line);
                }
                for (line_index, line) in code.lines().iter().enumerate() {
                    stats.chunks_visited += 1;
                    if let Some(source_line) = line.source_line {
                        code_lines.insert(
                            source_line,
                            (line.text.as_str(), highlights.line_shared(line_index).unwrap_or_default()),
                        );
                    }
                }
            }
        }
        let lines = std::mem::take(&mut self.line_ranges);
        let checkpoint = history.boundary.as_ref().and_then(|boundary| boundary.checkpoint.as_ref());
        let first_line = if checkpoint.is_some() { 0 } else { first_line.min(lines.len()) };
        let first_style = document.source_styles().partition_point(|style| style.source.bytes.start < first_byte);
        let mut styles_by_line: HashMap<usize, Vec<&MarkdownSourceStyle>> = HashMap::new();
        for style in &document.source_styles()[first_style..] {
            for line in style.source.lines.start..=style.source.lines.end {
                styles_by_line.entry(line).or_default().push(style);
            }
        }
        let first_target = document.targets().partition_point(|target| target.source.bytes.start < first_byte);
        let mut target_by_line: HashMap<usize, (usize, MarkdownTargetId)> = HashMap::new();
        for target in &document.targets()[first_target..] {
            for line in target.source.lines.start..=target.source.lines.end {
                let candidate = (target.source.bytes.len(), target.id);
                let slot = target_by_line.entry(line).or_insert(candidate);
                if candidate.0 < slot.0 {
                    *slot = candidate;
                }
            }
        }
        let base = Style::new().fg(page_color(theme, theme.diff.foreground));
        self.source_lines.truncate(first_line.min(self.source_lines.len()));
        let mut unit = Unit::default();
        for (index, range) in lines.iter().enumerate() {
            stats.chunks_visited += 1;
            if let Some(rows) = self.source_lines.get(index) {
                stats.rows_reused += rows.len();
                unit.push(rows, false);
                continue;
            }
            let raw = &source[range.bytes.clone()];
            let text = raw.strip_suffix('\n').unwrap_or(raw);
            let text = text.strip_suffix('\r').unwrap_or(text);
            let line_number = index + 1;
            let target = target_by_line.get(&line_number).map(|(_, id)| *id);
            let code_input = code_lines.get(&line_number);
            let styles = styles_by_line.get(&line_number).map_or(&[][..], Vec::as_slice);
            let mut output = RowOutput::new(options).after(checkpoint);
            let code = code_input.and_then(|(code, spans)| {
                Some((text.strip_suffix(code)?, highlighted_line(code, spans, &theme.syntax, code_style)))
            });
            let spans = match code {
                Some((prefix, line)) => {
                    let mut spans = vec![Span::styled(prefix.to_owned(), code_style)];
                    spans.extend(line.spans.iter().cloned());
                    spans
                }
                None => text
                    .grapheme_indices(true)
                    .map(|(offset, grapheme)| {
                        let position = range.bytes.start + offset;
                        let style = styles
                            .iter()
                            .filter(|style| style.source.bytes.contains(&position))
                            .fold(base, |style, role| source_role_style(role.role, style, theme));
                        Span::styled(grapheme.to_owned(), style)
                    })
                    .collect(),
            };
            output.push_wrapped(spans, "", RowOrigin { source: range, target });
            let chunk: RowChunk = output.rows.into();
            stats.rows_generated += chunk.len();
            self.source_lines.push(Arc::clone(&chunk));
            unit.push(&chunk, true);
        }
        self.source_lines.truncate(lines.len());
        self.line_ranges = lines;
        let skip = if checkpoint.is_some() { 0 } else { history.boundary.as_ref().map_or(0, |_| history.rows) };
        stats.row_store_updates += unit.store.take_updates();
        emit_unit(&unit, skip, store, first_generated);
        self.skipped_rows.push(skip.min(unit.len));
        self.block_ends.push(store.len());
        self.units.push(unit);
    }
}

fn emit_unit(unit: &Unit, skip: usize, store: &mut RowStore, first_generated: &mut Option<usize>) {
    if skip < unit.len {
        if let Some(generated) = unit.first_generated
            && first_generated.is_none()
        {
            *first_generated = Some(store.len() + generated.saturating_sub(skip));
        }
        store.append(&unit.store, skip..unit.len);
    }
}

/// Foreground and background for fenced code, composited over the page.
fn fenced_code_style(theme: &ReviewTheme) -> Style {
    layered_style(theme.markdown.code, theme.markdown.code_background, theme.diff.background)
}

/// Applies the inline-code role on top of `style`.
fn inline_code_style(style: Style, theme: &ReviewTheme) -> Style {
    style.patch(layered_style(theme.markdown.inline_code, theme.markdown.inline_code_background, theme.diff.background))
}

/// Ownership and styling a block inherits from its enclosing blocks.
#[derive(Clone, Copy)]
struct BlockContext<'a> {
    target: Option<MarkdownTargetId>,
    foreground: Rgba,
    prefix: &'a str,
}

/// Where the rows produced for one block element come from.
#[derive(Clone, Copy)]
struct RowOrigin<'a> {
    source: &'a SourceRange,
    target: Option<MarkdownTargetId>,
}

struct RowOutput {
    rows: Vec<Arc<MarkdownRow>>,
    options: MarkdownLayoutOptions,
    checkpoint: Option<RowCheckpoint>,
}

impl RowOutput {
    const fn new(options: MarkdownLayoutOptions) -> Self {
        Self { rows: Vec::new(), options, checkpoint: None }
    }

    fn after(mut self, checkpoint: Option<&RowCheckpoint>) -> Self {
        self.checkpoint = checkpoint.cloned();
        self
    }

    fn push(&mut self, line: Line<'static>, origin: RowOrigin<'_>) {
        self.rows.push(Arc::new(MarkdownRow {
            line,
            source: Some(origin.source.clone()),
            target: origin.target,
            checkpoint: None,
        }));
    }

    fn push_wrapped(&mut self, spans: Vec<Span<'static>>, continuation: &str, origin: RowOrigin<'_>) {
        if self.checkpoint.as_ref().is_some_and(|checkpoint| origin.source.bytes.end <= checkpoint.source.bytes.start) {
            return;
        }
        let rendered: Arc<str> = spans.iter().map(|span| span.content.as_ref()).collect::<String>().into();
        let mut from = FitPosition::default();
        if let Some(checkpoint) = &self.checkpoint {
            if origin.source.bytes.end <= checkpoint.source.bytes.start {
                return;
            }
            if origin.source.bytes.start <= checkpoint.source.bytes.start {
                from = checkpoint.position;
                if !rendered.starts_with(checkpoint.rendered.as_ref()) {
                    from.byte = translated_offset(&checkpoint.rendered, &rendered, from.byte);
                    from.tab_remaining = 0;
                }
                if from.byte >= rendered.len() {
                    return;
                }
            }
        }
        let options = self.fit_options(self.options.width, continuation);
        for (line, position) in fit_spans_from(spans, options, from) {
            self.rows.push(Arc::new(MarkdownRow {
                line,
                source: Some(origin.source.clone()),
                target: origin.target,
                checkpoint: Some(RowCheckpoint {
                    source: origin.source.clone(),
                    position,
                    rendered: Arc::clone(&rendered),
                }),
            }));
        }
    }

    fn fit_options<'a>(&self, width: u16, continuation: &'a str) -> FitOptions<'a> {
        FitOptions {
            width: usize::from(width),
            wrap: self.options.wrap,
            tab_width: usize::from(self.options.tab_width),
            continuation,
        }
    }
}

fn translated_offset(before: &str, after: &str, offset: usize) -> usize {
    let mut old = 0;
    let mut new = 0;
    for change in TextDiff::from_chars(before, after).iter_all_changes() {
        if old >= offset {
            break;
        }
        match change.tag() {
            ChangeTag::Equal => {
                old += change.value().len();
                new += change.value().len();
            }
            ChangeTag::Delete => old += change.value().len(),
            ChangeTag::Insert => new += change.value().len(),
        }
    }
    new
}

fn render_block(block: &MarkdownBlock, theme: &ReviewTheme, output: &mut RowOutput, context: BlockContext<'_>) {
    let prefix = context.prefix;
    let width = output.options.width;
    let origin = RowOrigin { source: &block.source, target: block.target_id.or(context.target) };
    match &block.kind {
        MarkdownBlockKind::Heading { level, content } => {
            let marker = if output.options.heading_markers {
                format!("{} ", "#".repeat(usize::from(*level)))
            } else {
                String::new()
            };
            let base = Style::new().fg(page_color(theme, theme.markdown.heading)).add_modifier(Modifier::BOLD);
            let mut spans = vec![Span::styled(format!("{prefix}{marker}"), base)];
            spans.extend(inline_spans(content, base, theme));
            output.push_wrapped(spans, prefix, origin);
        }
        MarkdownBlockKind::Paragraph { content } | MarkdownBlockKind::HtmlFallback { content } => {
            let base = Style::new().fg(page_color(theme, context.foreground));
            let mut spans = vec![Span::styled(prefix.to_owned(), base)];
            spans.extend(inline_spans(content, base, theme));
            output.push_wrapped(spans, prefix, origin);
        }
        MarkdownBlockKind::List { ordered, start, items } => {
            render_list(items, (*ordered, *start), theme, output, BlockContext { target: origin.target, ..context });
        }
        MarkdownBlockKind::BlockQuote { blocks } => {
            let quote_prefix = format!("{prefix}│ ");
            for child in blocks {
                render_block(
                    child,
                    theme,
                    output,
                    BlockContext { target: origin.target, foreground: theme.markdown.quote, prefix: &quote_prefix },
                );
            }
        }
        MarkdownBlockKind::CodeBlock(code) => {
            render_code(code, theme, output, origin.target, prefix);
        }
        MarkdownBlockKind::Table(table) => {
            render_table(table, theme, output, context.foreground, origin.target, prefix);
        }
        MarkdownBlockKind::Rule => output.push(
            Line::styled("─".repeat(usize::from(width)), Style::new().fg(page_color(theme, theme.diff.border))),
            origin,
        ),
    }
}

fn render_list(
    items: &[MarkdownListItem],
    (ordered, start): (bool, Option<u64>),
    theme: &ReviewTheme,
    output: &mut RowOutput,
    context: BlockContext<'_>,
) {
    let prefix = context.prefix;
    let base = Style::new().fg(page_color(theme, context.foreground));
    for (index, item) in items.iter().enumerate() {
        let item_target = item.target_id.or(context.target);
        let marker =
            if ordered { format!("{}.", start.unwrap_or(1).saturating_add(index as u64)) } else { "•".to_owned() };
        let mut spans = vec![Span::styled(format!("{prefix}{}{marker} ", "  ".repeat(item.depth)), base)];
        spans.extend(inline_spans(&item.content, base, theme));
        let continuation = format!("{prefix}{}", " ".repeat(marker.width() + 1));
        output.push_wrapped(spans, &continuation, RowOrigin { source: &item.source, target: item_target });
        let child_prefix = format!("{prefix}  ");
        for child in &item.blocks {
            render_block(child, theme, output, BlockContext { target: item_target, prefix: &child_prefix, ..context });
        }
    }
}

fn render_code(
    code: &MarkdownCodeBlock,
    theme: &ReviewTheme,
    output: &mut RowOutput,
    target: Option<MarkdownTargetId>,
    prefix: &str,
) {
    let highlights = &code.syntax().highlights;
    let base = fenced_code_style(theme);
    for (index, line) in code.lines().iter().enumerate() {
        render_code_line(
            line,
            highlights.line(index).unwrap_or_default(),
            &theme.syntax,
            base,
            prefix,
            line.target_id.or(target),
            output,
        );
    }
}

fn render_code_line(
    line: &clankerdiff_markdown::MarkdownCodeLine,
    spans: &[HighlightSpan],
    syntax: &SyntaxTheme,
    base: Style,
    prefix: &str,
    target: Option<MarkdownTargetId>,
    output: &mut RowOutput,
) {
    let mut rendered = highlighted_line(&line.text, spans, syntax, base);
    rendered.spans.insert(0, Span::styled(prefix.to_owned(), base));
    output.push_wrapped(rendered.spans, prefix, RowOrigin { source: &line.source, target });
}

/// Column widths for `table`, or `None` when the columns cannot fit side by
/// side and cells must stack.
fn table_column_widths(table: &MarkdownTable, available: usize, wrap: bool) -> Option<Vec<usize>> {
    let columns = table_columns(table);
    let mut natural = vec![1; columns];
    for row in &table.rows {
        for (index, cell) in row.cells.iter().enumerate() {
            natural[index] = natural[index].max(rendered_text(&cell.content).width());
        }
    }
    if !wrap || natural.iter().sum::<usize>() <= available {
        return Some(natural);
    }
    if available < columns {
        return None;
    }
    let mut order = (0..columns).collect::<Vec<_>>();
    order.sort_by_key(|index| natural[*index]);
    let mut widths = vec![0; columns];
    let mut budget = available;
    for (rank, index) in order.into_iter().enumerate() {
        let share = budget / (columns - rank);
        widths[index] = natural[index].min(share);
        budget -= widths[index];
    }
    Some(widths)
}

fn table_columns(table: &MarkdownTable) -> usize {
    table.rows.iter().map(|row| row.cells.len()).max().unwrap_or(0)
}

fn render_table(
    table: &MarkdownTable,
    theme: &ReviewTheme,
    output: &mut RowOutput,
    foreground: Rgba,
    target: Option<MarkdownTargetId>,
    prefix: &str,
) {
    let text = Style::new().fg(page_color(theme, foreground));
    let border = Style::new().fg(page_color(theme, theme.diff.border));
    let columns = table_columns(table);
    let available = usize::from(output.options.width).saturating_sub(prefix.width() + columns * 3 + 1);
    let widths = table_column_widths(table, available, output.options.wrap);
    for row in &table.rows {
        let base = if row.header { text.add_modifier(Modifier::BOLD) } else { text };
        let origin = RowOrigin { source: &row.source, target: row.target_id.or(target) };
        let Some(widths) = &widths else {
            for cell in &row.cells {
                output.push_wrapped(inline_spans(&cell.content, base, theme), prefix, origin);
            }
            continue;
        };
        if widths.is_empty() {
            continue;
        }
        let cells = row
            .cells
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                fit_spans_from(
                    inline_spans(&cell.content, base, theme),
                    output.fit_options(u16::try_from(widths[index]).unwrap_or(u16::MAX), ""),
                    FitPosition::default(),
                )
                .map(|(line, _)| line)
                .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let height = cells.iter().map(Vec::len).max().unwrap_or(1);
        for line in 0..height {
            let mut spans = vec![Span::styled(format!("{prefix}│ "), border)];
            for (index, cell_width) in widths.iter().copied().enumerate() {
                if index > 0 {
                    spans.push(Span::styled(" │ ", border));
                }
                let cell = cells.get(index).and_then(|rows| rows.get(line));
                let padding = cell_width.saturating_sub(cell.map_or(0, Line::width));
                let left = match table.alignments.get(index) {
                    Some(MarkdownTableAlignment::Right) => padding,
                    Some(MarkdownTableAlignment::Center) => padding / 2,
                    _ => 0,
                };
                spans.push(Span::styled(" ".repeat(left), base));
                if let Some(cell) = cell {
                    spans.extend(cell.spans.iter().cloned());
                }
                spans.push(Span::styled(" ".repeat(padding - left), base));
            }
            spans.push(Span::styled(" │", border));
            output.push_wrapped(spans, prefix, origin);
        }
    }
}

fn inline_spans(inlines: &[MarkdownInline], style: Style, theme: &ReviewTheme) -> Vec<Span<'static>> {
    fn append(inline: &MarkdownInline, style: Style, theme: &ReviewTheme, output: &mut Vec<Span<'static>>) {
        match inline {
            MarkdownInline::Text(text) => output.push(Span::styled(text.clone(), style)),
            MarkdownInline::Code(text) => {
                output.push(Span::styled(text.clone(), inline_code_style(style, theme)));
            }
            MarkdownInline::Strong(children) => children.iter().for_each(|child| {
                append(child, style.add_modifier(Modifier::BOLD), theme, output);
            }),
            MarkdownInline::Emphasis(children) => children.iter().for_each(|child| {
                append(child, style.add_modifier(Modifier::ITALIC), theme, output);
            }),
            MarkdownInline::Strikethrough(children) => children.iter().for_each(|child| {
                append(child, style.add_modifier(Modifier::CROSSED_OUT), theme, output);
            }),
            MarkdownInline::Link { content, .. } => content.iter().for_each(|child| {
                append(
                    child,
                    style.fg(page_color(theme, theme.markdown.link)).add_modifier(Modifier::UNDERLINED),
                    theme,
                    output,
                );
            }),
            MarkdownInline::SoftBreak => output.push(Span::styled(" ", style)),
            MarkdownInline::HardBreak => output.push(Span::styled("\n", style)),
            MarkdownInline::ImageAlt(text) => output.push(Span::styled(
                format!("Image: {text}"),
                style.fg(page_color(theme, theme.markdown.link)).add_modifier(Modifier::ITALIC),
            )),
        }
    }

    let mut output = Vec::new();
    for inline in inlines {
        append(inline, style, theme, &mut output);
    }
    output
}

fn source_role_style(role: MarkdownSourceRole, style: Style, theme: &ReviewTheme) -> Style {
    match role {
        MarkdownSourceRole::Heading => style.fg(page_color(theme, theme.markdown.heading)).add_modifier(Modifier::BOLD),
        MarkdownSourceRole::Link => style.fg(page_color(theme, theme.markdown.link)).add_modifier(Modifier::UNDERLINED),
        MarkdownSourceRole::Quote => style.fg(page_color(theme, theme.markdown.quote)),
        MarkdownSourceRole::Code => inline_code_style(style, theme),
        MarkdownSourceRole::Strong => style.add_modifier(Modifier::BOLD),
        MarkdownSourceRole::Emphasis => style.add_modifier(Modifier::ITALIC),
        MarkdownSourceRole::Strikethrough => style.add_modifier(Modifier::CROSSED_OUT),
    }
}
