//! Framework-neutral, random-access unified and split row presentation.

use crate::{
    DiffDocument, DiffSide, FileDiff, FileStatus, Fingerprint, Hunk, LineAnchor, PatchLine,
    PatchLineKind, RepoPath, SourceDocument, SourceKey, SourceLineRef, SourceLocation,
    SourceSequenceId, SourceUnavailable,
};
use serde::{Deserialize, Serialize};
use similar::{DiffOp, TextDiff};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::{Arc, OnceLock},
};

const NO_NEWLINE_TEXT: &str = "\\ No newline at end of file";

/// Largest hunk (in total patch lines) served as a fallback syntax sequence.
/// Larger hunks degrade to per-line highlighting so patch-only rendering work
/// stays bounded by the viewport rather than the document.
pub const MAX_HUNK_SEQUENCE_LINES: usize = 512;

/// Requested diff layout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    /// Let an adapter choose based on its own width units.
    #[default]
    Auto,
    Unified,
    Split,
}

impl ViewMode {
    #[must_use]
    pub const fn resolve(self, split_when_auto: bool) -> Layout {
        match self {
            Self::Split => Layout::Split,
            Self::Auto if split_when_auto => Layout::Split,
            Self::Unified | Self::Auto => Layout::Unified,
        }
    }

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Auto => Self::Unified,
            Self::Unified => Self::Split,
            Self::Split => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Layout {
    /// One code column.
    Unified,
    /// Old and new columns aligned side by side.
    Split,
}

impl Layout {
    #[must_use]
    pub const fn is_split(self) -> bool {
        matches!(self, Self::Split)
    }
}

/// Options used while indexing presentation rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentationOptions {
    /// Requested layout mode.
    pub view_mode: ViewMode,
    /// How `Auto` is resolved by the calling adapter.
    pub split_when_auto: bool,
    /// Whether each file begins with a file header row.
    pub include_file_headers: bool,
}

impl Default for PresentationOptions {
    fn default() -> Self {
        Self {
            view_mode: ViewMode::Auto,
            split_when_auto: false,
            include_file_headers: true,
        }
    }
}

impl PresentationOptions {
    /// Resolves the requested mode to a concrete layout.
    #[must_use]
    pub const fn layout(self) -> Layout {
        self.view_mode.resolve(self.split_when_auto)
    }
}

/// Semantic row type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RowKind {
    FileHeader,
    HunkHeader,
    Meta,
    Code,
    ExpandedContext,
    ExpandGap,
}

/// Stable identity of a gap before, between, or after hunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GapId {
    pub file_index: usize,
    pub gap_index: usize,
}

/// User-controlled revealed ranges at both hunk-adjacent edges of a gap.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GapExpansion {
    pub revealed_prefix: usize,
    pub revealed_suffix: usize,
}

/// One-based half-open source intervals for both sides of a gap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GapInterval {
    pub old: Range<usize>,
    pub new: Range<usize>,
}

impl GapInterval {
    #[must_use]
    pub fn sources_match(&self, old: &SourceDocument, new: &SourceDocument) -> bool {
        self.old.len() == self.new.len()
            && self
                .old
                .clone()
                .zip(self.new.clone())
                .all(|(old_line, new_line)| old.line(old_line) == new.line(new_line))
    }
}

/// Adapter-facing state for an expansion affordance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GapInfo {
    pub id: GapId,
    pub hidden_lines: usize,
    pub unavailable: Option<SourceUnavailable>,
}

impl GapInfo {
    /// Returns the deterministic renderer-independent gap label.
    #[must_use]
    pub fn message(&self) -> String {
        match &self.unavailable {
            None => format!("⋯ {} unchanged lines", self.hidden_lines),
            Some(SourceUnavailable::TooLarge { .. }) => {
                "⋯ source is too large to expand".to_owned()
            }
            Some(reason) => format!("⋯ {reason}"),
        }
    }
}

/// Complete source and reveal state used to project a document.
#[derive(Debug, Clone, Default)]
pub struct ContentProjection {
    pub(crate) sources: HashMap<SourceKey, Result<Arc<SourceDocument>, SourceUnavailable>>,
    pub(crate) expansions: HashMap<GapId, GapExpansion>,
    pub(crate) full_files: HashSet<RepoPath>,
}

impl ContentProjection {
    #[must_use]
    pub fn with_sources(
        sources: HashMap<SourceKey, Result<Arc<SourceDocument>, SourceUnavailable>>,
    ) -> Self {
        Self {
            sources,
            ..Self::default()
        }
    }

    pub fn insert_source(&mut self, key: SourceKey, source: Arc<SourceDocument>) {
        self.sources.insert(key, Ok(source));
    }

    pub fn insert_unavailable(&mut self, key: SourceKey, reason: SourceUnavailable) {
        self.sources.insert(key, Err(reason));
    }

    pub fn set_expansion(&mut self, id: GapId, expansion: GapExpansion) {
        self.expansions.insert(id, expansion);
    }

    pub fn set_full_file(&mut self, path: RepoPath, enabled: bool) {
        if enabled {
            self.full_files.insert(path);
        } else {
            self.full_files.remove(&path);
        }
    }

    #[must_use]
    pub fn source(&self, key: &SourceKey) -> Option<&Arc<SourceDocument>> {
        self.sources.get(key)?.as_ref().ok()
    }

    #[must_use]
    pub fn unavailable(&self, key: &SourceKey) -> Option<&SourceUnavailable> {
        self.sources.get(key)?.as_ref().err()
    }

    #[must_use]
    pub fn is_full_file(&self, path: &RepoPath) -> bool {
        self.full_files.contains(path)
    }
}

pub use diff_theme::DiffTone;

/// Renderer-neutral description of the hunk-side line sequence containing a
/// patch cell, used to keep multiline syntax context for patch-only files.
pub struct HunkSequence<'a> {
    /// Content-derived, snapshot-local cache identity for this side's lines.
    pub id: SourceSequenceId,
    /// Side-specific repository path usable as a syntax language hint.
    pub path: &'a str,
    /// Zero-based index of the cell's line within [`Self::lines`].
    pub target_line: usize,
    hunk: &'a Hunk,
    side: DiffSide,
}

impl HunkSequence<'_> {
    /// This side's hunk lines in source order.
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.hunk
            .lines
            .iter()
            .filter(|line| line.line_number(self.side).is_some())
            .map(|line| line.text.as_ref())
    }
}

fn hunk_side_sequence_id(lines: &[PatchLine], side: DiffSide) -> SourceSequenceId {
    SourceSequenceId::from_lines(
        lines
            .iter()
            .filter(|line| line.line_number(side).is_some())
            .map(|line| line.text.as_ref()),
    )
}

/// Stable identity for a presented row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RowId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CellSource {
    pub side: DiffSide,
    pub hunk_index: usize,
    pub line_index: usize,
}

/// One side of a presented row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentedCell {
    pub patch_source: Option<CellSource>,
    pub source_line: Option<SourceLineRef>,
    pub text: Arc<str>,
    pub tone: DiffTone,
}

/// One cheap renderer-neutral row descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentedRow {
    pub id: RowId,
    pub kind: RowKind,
    pub file_index: usize,
    pub hunk_index: Option<usize>,
    pub left: Option<PresentedCell>,
    pub right: Option<PresentedCell>,
}

impl PresentedCell {
    #[must_use]
    pub fn line_number(&self) -> Option<usize> {
        self.source_line.map(|source| source.line_number)
    }
}

impl PresentedRow {
    #[must_use]
    pub const fn cell(&self, side: DiffSide) -> Option<&PresentedCell> {
        match side {
            DiffSide::Old => self.left.as_ref(),
            DiffSide::New => self.right.as_ref(),
        }
    }

    #[must_use]
    pub const fn primary_cell(&self) -> Option<&PresentedCell> {
        match &self.right {
            Some(cell) => Some(cell),
            None => self.left.as_ref(),
        }
    }

    #[must_use]
    pub const fn preferred_cell(&self, side: DiffSide) -> Option<&PresentedCell> {
        match self.cell(side) {
            Some(cell) => Some(cell),
            None => self.primary_cell(),
        }
    }

    pub fn cells(&self) -> impl Iterator<Item = &PresentedCell> {
        [self.left.as_ref(), self.right.as_ref()]
            .into_iter()
            .flatten()
    }

    pub fn sources(&self) -> impl Iterator<Item = CellSource> + '_ {
        self.cells().filter_map(|cell| cell.patch_source)
    }

    pub fn source_lines(&self) -> impl Iterator<Item = SourceLineRef> + '_ {
        self.cells().filter_map(|cell| cell.source_line)
    }

    #[must_use]
    pub fn is_commentable(&self) -> bool {
        self.kind == RowKind::Code && self.sources().next().is_some()
    }

    #[must_use]
    pub fn is_navigable(&self) -> bool {
        match self.kind {
            RowKind::Code | RowKind::ExpandedContext => self.source_lines().next().is_some(),
            RowKind::ExpandGap => true,
            RowKind::FileHeader | RowKind::HunkHeader | RowKind::Meta => false,
        }
    }
}

/// Eager row indexes and cheap descriptors. Syntax and frontend widgets are not
/// constructed here.
#[derive(Debug, Clone)]
pub struct DiffPresentation {
    document: Arc<DiffDocument>,
    layout: Layout,
    rows: Vec<PresentedRow>,
    anchor_rows: HashMap<(usize, DiffSide, usize), usize>,
    source_rows: HashMap<(usize, DiffSide, usize), usize>,
    gap_info: HashMap<usize, GapInfo>,
    sources: HashMap<SourceKey, Result<Arc<SourceDocument>, SourceUnavailable>>,
    file_ranges: Vec<Range<usize>>,
    hunk_ranges: Vec<Vec<Range<usize>>>,
    /// Lazily fingerprinted per hunk so presentations that never fall back to
    /// hunk-sequence highlighting skip hashing the document.
    sequence_ids: Vec<Vec<OnceLock<[SourceSequenceId; 2]>>>,
}

impl DiffPresentation {
    /// Indexes a document once for O(1) row lookup and slicing.
    #[must_use]
    pub fn new(document: Arc<DiffDocument>, options: PresentationOptions) -> Self {
        Self::with_sources(document, options, &ContentProjection::default())
    }

    /// Builds a windowed projection over optional immutable complete-file sources.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn with_sources(
        document: Arc<DiffDocument>,
        options: PresentationOptions,
        projection: &ContentProjection,
    ) -> Self {
        let layout = options.layout();
        let mut rows = Vec::new();
        let mut gap_info = HashMap::new();
        let mut file_ranges = Vec::with_capacity(document.files.len());
        let mut hunk_ranges = Vec::with_capacity(document.files.len());
        for (file_index, file) in document.files.iter().enumerate() {
            let file_start = rows.len();
            if options.include_file_headers {
                rows.push(header_row(file_index, file));
            }
            let old_key = SourceKey::new(file.path.clone(), DiffSide::Old);
            let new_key = SourceKey::new(file.path.clone(), DiffSide::New);
            let old_count = projection
                .source(&old_key)
                .map_or(0, |source| source.line_count());
            let new_count = projection
                .source(&new_key)
                .map_or(0, |source| source.line_count());
            let gaps = gaps_for_file(file, old_count, new_count);
            let mut file_hunks = Vec::with_capacity(file.hunks.len());
            for (hunk_index, hunk) in file.hunks.iter().enumerate() {
                append_gap_projection(
                    &mut rows,
                    &mut gap_info,
                    GapProjection {
                        file_index,
                        gap_index: hunk_index,
                        file,
                        gap: &gaps[hunk_index],
                        layout,
                        projection,
                    },
                );
                let hunk_start = rows.len();
                rows.push(hunk_header_row(file_index, hunk_index, hunk, &file.path));
                match layout {
                    Layout::Unified => append_unified_rows(&mut rows, file_index, hunk_index, file),
                    Layout::Split => append_split_rows(&mut rows, file_index, hunk_index, file),
                }
                file_hunks.push(hunk_start..rows.len());
            }
            if file.hunks.is_empty() {
                if projection.is_full_file(&file.path) {
                    append_gap_projection(
                        &mut rows,
                        &mut gap_info,
                        GapProjection {
                            file_index,
                            gap_index: 0,
                            file,
                            gap: &gaps[0],
                            layout,
                            projection,
                        },
                    );
                } else {
                    rows.push(meta_row(
                        file_index,
                        None,
                        &file.path,
                        "placeholder",
                        None,
                        placeholder_text(file),
                    ));
                }
            } else {
                append_gap_projection(
                    &mut rows,
                    &mut gap_info,
                    GapProjection {
                        file_index,
                        gap_index: file.hunks.len(),
                        file,
                        gap: &gaps[file.hunks.len()],
                        layout,
                        projection,
                    },
                );
            }
            file_ranges.push(file_start..rows.len());
            hunk_ranges.push(file_hunks);
        }
        let anchor_rows = rows
            .iter()
            .enumerate()
            .flat_map(|(row_index, row)| {
                row.cells().filter_map(move |cell| {
                    let source = cell.patch_source?;
                    Some((
                        (row.file_index, source.side, cell.line_number()?),
                        row_index,
                    ))
                })
            })
            .collect();
        let source_rows = rows
            .iter()
            .enumerate()
            .flat_map(|(row_index, row)| {
                row.cells().filter_map(move |cell| {
                    let source = cell.source_line?;
                    Some(((row.file_index, source.side, source.line_number), row_index))
                })
            })
            .collect();
        let sequence_ids = document
            .files
            .iter()
            .map(|file| file.hunks.iter().map(|_| OnceLock::new()).collect())
            .collect();
        Self {
            document,
            layout,
            rows,
            anchor_rows,
            source_rows,
            gap_info,
            sources: projection.sources.clone(),
            file_ranges,
            hunk_ranges,
            sequence_ids,
        }
    }

    /// Returns the retained immutable snapshot.
    #[must_use]
    pub const fn document(&self) -> &Arc<DiffDocument> {
        &self.document
    }

    #[must_use]
    pub const fn layout(&self) -> Layout {
        self.layout
    }

    /// Total number of rows.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Returns one row in O(1).
    #[must_use]
    pub fn row(&self, index: usize) -> Option<&PresentedRow> {
        self.rows.get(index)
    }

    /// Returns a clamped visible row slice without allocating.
    #[must_use]
    pub fn rows(&self, range: Range<usize>) -> &[PresentedRow] {
        let start = range.start.min(self.rows.len());
        let end = range.end.max(start).min(self.rows.len());
        &self.rows[start..end]
    }

    /// Returns the row range occupied by a file.
    #[must_use]
    pub fn file_range(&self, file_index: usize) -> Option<Range<usize>> {
        self.file_ranges.get(file_index).cloned()
    }

    /// Returns the row range occupied by a hunk, including its header.
    #[must_use]
    pub fn hunk_range(&self, file_index: usize, hunk_index: usize) -> Option<Range<usize>> {
        self.hunk_ranges.get(file_index)?.get(hunk_index).cloned()
    }

    #[must_use]
    pub fn cell_anchor(&self, row: &PresentedRow, cell: &PresentedCell) -> Option<LineAnchor> {
        let source = cell.patch_source?;
        let file = self.document.files.get(row.file_index)?;
        LineAnchor::for_line(file, source.side, source.hunk_index, source.line_index)
    }

    #[must_use]
    pub fn anchor_at(&self, row_index: usize, side: DiffSide) -> Option<LineAnchor> {
        let row = self.row(row_index)?;
        self.cell_anchor(row, row.preferred_cell(side)?)
    }

    /// Returns the immutable complete source containing `cell`.
    #[must_use]
    pub fn source_document(
        &self,
        row: &PresentedRow,
        cell: &PresentedCell,
    ) -> Option<&SourceDocument> {
        let source = cell.source_line?;
        let file = self.document.files.get(row.file_index)?;
        let key = SourceKey::new(file.path.clone(), source.side);
        self.sources.get(&key)?.as_ref().ok().map(AsRef::as_ref)
    }

    /// Returns the side-specific repository path used as a syntax language hint.
    #[must_use]
    pub fn source_path<'a>(&'a self, row: &PresentedRow, cell: &PresentedCell) -> Option<&'a str> {
        let source = cell.source_line?;
        Some(
            self.document
                .files
                .get(row.file_index)?
                .path_for_side(source.side)
                .as_str(),
        )
    }

    /// Describes the hunk-side line sequence containing a patch cell so hosts
    /// can keep multiline syntax context when no complete source exists.
    #[must_use]
    pub fn hunk_sequence<'a>(
        &'a self,
        row: &PresentedRow,
        cell: &PresentedCell,
    ) -> Option<HunkSequence<'a>> {
        let source = cell.patch_source?;
        let file = self.document.files.get(row.file_index)?;
        let hunk = file.hunks.get(source.hunk_index)?;
        if hunk.lines.len() > MAX_HUNK_SEQUENCE_LINES {
            return None;
        }
        hunk.lines.get(source.line_index)?;
        let target_line = hunk.lines[..source.line_index]
            .iter()
            .filter(|line| line.line_number(source.side).is_some())
            .count();
        let ids = self
            .sequence_ids
            .get(row.file_index)?
            .get(source.hunk_index)?
            .get_or_init(|| {
                [
                    hunk_side_sequence_id(&hunk.lines, DiffSide::Old),
                    hunk_side_sequence_id(&hunk.lines, DiffSide::New),
                ]
            });
        Some(HunkSequence {
            id: match source.side {
                DiffSide::Old => ids[0],
                DiffSide::New => ids[1],
            },
            path: file.path_for_side(source.side).as_str(),
            target_line,
            hunk,
            side: source.side,
        })
    }

    #[must_use]
    pub fn language_at(&self, row_index: usize) -> &str {
        self.row(row_index)
            .map_or("", |row| self.language_at_row(row))
    }

    /// Repository path of the file a row belongs to, usable as a language hint.
    #[must_use]
    pub fn row_path(&self, row: &PresentedRow) -> &str {
        self.document
            .files
            .get(row.file_index)
            .map_or("", |file| file.path.as_str())
    }

    fn language_at_row(&self, row: &PresentedRow) -> &str {
        self.document
            .files
            .get(row.file_index)
            .map_or("", FileDiff::language)
    }

    /// Returns the presentation row displaying an anchor in O(1) after its file
    /// has been located.
    #[must_use]
    pub fn row_showing_anchor(&self, anchor: &LineAnchor) -> Option<usize> {
        let file_index = self.document.file_index(&anchor.path)?;
        self.anchor_rows
            .get(&(file_index, anchor.side, anchor.line_number()?))
            .copied()
    }

    #[must_use]
    pub fn source_location(
        &self,
        row: &PresentedRow,
        cell: &PresentedCell,
    ) -> Option<SourceLocation> {
        let source = cell.source_line?;
        Some(SourceLocation {
            path: self.document.files.get(row.file_index)?.path.clone(),
            side: source.side,
            line_number: source.line_number,
        })
    }

    #[must_use]
    pub fn row_showing_source(&self, location: &SourceLocation) -> Option<usize> {
        let file_index = self.document.file_index(&location.path)?;
        self.source_rows
            .get(&(file_index, location.side, location.line_number))
            .copied()
    }

    #[must_use]
    pub fn gap_info(&self, row_index: usize) -> Option<&GapInfo> {
        self.gap_info.get(&row_index)
    }

    #[must_use]
    pub fn row_shows_anchor(&self, row: &PresentedRow, anchor: &LineAnchor) -> bool {
        self.document
            .files
            .get(row.file_index)
            .is_some_and(|file| file.path == anchor.path)
            && row.cells().any(|cell| {
                cell.patch_source
                    .is_some_and(|source| source.side == anchor.side)
                    && cell.line_number() == anchor.line_number()
            })
    }

    #[must_use]
    pub fn is_commentable(&self, index: usize) -> bool {
        self.row(index).is_some_and(PresentedRow::is_commentable)
    }

    #[must_use]
    pub fn is_navigable(&self, index: usize) -> bool {
        self.row(index).is_some_and(PresentedRow::is_navigable)
    }

    #[must_use]
    pub fn first_navigable(&self, range: Range<usize>) -> Option<usize> {
        range.into_iter().find(|index| self.is_navigable(*index))
    }

    #[must_use]
    pub fn last_navigable(&self, range: Range<usize>) -> Option<usize> {
        range.rev().find(|index| self.is_navigable(*index))
    }

    #[must_use]
    pub fn step_navigable(
        &self,
        from: usize,
        backward: bool,
        range: &Range<usize>,
    ) -> Option<usize> {
        if backward {
            self.last_navigable(range.start..from.min(range.end))
        } else {
            self.first_navigable(from.saturating_add(1).max(range.start)..range.end)
        }
    }
}

/// Computes leading, between-hunk, and trailing one-based source intervals.
#[must_use]
pub fn gaps_for_file(
    file: &FileDiff,
    old_line_count: usize,
    new_line_count: usize,
) -> Vec<GapInterval> {
    if file.hunks.is_empty() {
        return vec![GapInterval {
            old: 1..old_line_count.saturating_add(1),
            new: 1..new_line_count.saturating_add(1),
        }];
    }
    let boundary = |start: usize, count: usize| {
        if count == 0 {
            start.saturating_add(1)
        } else {
            start
        }
    };
    let after = |start: usize, count: usize| {
        if count == 0 {
            start.saturating_add(1)
        } else {
            start.saturating_add(count)
        }
    };
    let mut gaps = Vec::with_capacity(file.hunks.len().saturating_add(1));
    let (mut old_next, mut new_next) = (1, 1);
    for hunk in &file.hunks {
        let old_end = boundary(hunk.old_start, hunk.old_count).max(old_next);
        let new_end = boundary(hunk.new_start, hunk.new_count).max(new_next);
        gaps.push(GapInterval {
            old: old_next..old_end,
            new: new_next..new_end,
        });
        old_next = after(hunk.old_start, hunk.old_count).max(old_end);
        new_next = after(hunk.new_start, hunk.new_count).max(new_end);
    }
    gaps.push(GapInterval {
        old: old_next..old_line_count.saturating_add(1).max(old_next),
        new: new_next..new_line_count.saturating_add(1).max(new_next),
    });
    gaps
}

#[derive(Clone, Copy)]
struct GapProjection<'a> {
    file_index: usize,
    gap_index: usize,
    file: &'a FileDiff,
    gap: &'a GapInterval,
    layout: Layout,
    projection: &'a ContentProjection,
}

fn append_gap_projection(
    rows: &mut Vec<PresentedRow>,
    gap_info: &mut HashMap<usize, GapInfo>,
    projection_args: GapProjection<'_>,
) {
    let GapProjection {
        file_index,
        gap_index,
        file,
        gap,
        layout,
        projection,
    } = projection_args;
    let id = GapId {
        file_index,
        gap_index,
    };
    let expansion = projection.expansions.get(&id).copied();
    let full_file = projection.is_full_file(&file.path);
    if expansion.is_none() && !full_file {
        return;
    }
    let expansion = expansion.unwrap_or_default();
    let old_key = SourceKey::new(file.path.clone(), DiffSide::Old);
    let new_key = SourceKey::new(file.path.clone(), DiffSide::New);
    let old = projection.source(&old_key);
    let new = projection.source(&new_key);
    let total = gap.old.len().max(gap.new.len());
    let loaded = gap_required_sides(layout, file.status)
        .iter()
        .all(|side| match side {
            DiffSide::Old => old.is_some(),
            DiffSide::New => new.is_some(),
        });
    let prefix = if full_file {
        total
    } else {
        expansion.revealed_prefix.min(total)
    };
    let suffix = if full_file {
        total
    } else {
        expansion.revealed_suffix.min(total.saturating_sub(prefix))
    };
    let hidden = if loaded {
        total.saturating_sub(prefix.saturating_add(suffix))
    } else {
        total
    };
    let expanded = ExpandedRow {
        file_index,
        file,
        gap,
        layout,
        old,
        new,
    };

    if loaded {
        for offset in 0..prefix {
            append_expanded_row(rows, expanded, offset);
        }
    }
    if hidden != 0 || !loaded {
        let unavailable = gap_unavailable(file, layout, projection, &old_key, &new_key);
        let row_index = rows.len();
        rows.push(PresentedRow {
            id: row_id(&file.path, "expand-gap", Some(gap_index), None, None),
            kind: RowKind::ExpandGap,
            file_index,
            hunk_index: None,
            left: None,
            right: Some(cell(
                None,
                None,
                Arc::<str>::from("unchanged lines"),
                DiffTone::Meta,
            )),
        });
        gap_info.insert(
            row_index,
            GapInfo {
                id,
                hidden_lines: hidden,
                unavailable,
            },
        );
    }
    if loaded {
        let start = total.saturating_sub(suffix).max(prefix);
        for offset in start..total {
            append_expanded_row(rows, expanded, offset);
        }
    }
}

fn gap_required_sides(layout: Layout, status: FileStatus) -> &'static [DiffSide] {
    match (layout, status) {
        (Layout::Split | Layout::Unified, FileStatus::Deleted) => &[DiffSide::Old],
        (Layout::Split, FileStatus::Added | FileStatus::Untracked) | (Layout::Unified, _) => {
            &[DiffSide::New]
        }
        (Layout::Split, _) => &[DiffSide::Old, DiffSide::New],
    }
}

fn gap_unavailable(
    file: &FileDiff,
    layout: Layout,
    projection: &ContentProjection,
    old_key: &SourceKey,
    new_key: &SourceKey,
) -> Option<SourceUnavailable> {
    gap_required_sides(layout, file.status)
        .iter()
        .find_map(|side| {
            let key = match side {
                DiffSide::Old => old_key,
                DiffSide::New => new_key,
            };
            match projection.sources.get(key) {
                Some(Ok(_)) => None,
                Some(Err(reason)) => Some(reason.clone()),
                None => Some(SourceUnavailable::Absent),
            }
        })
}

#[derive(Clone, Copy)]
struct ExpandedRow<'a> {
    file_index: usize,
    file: &'a FileDiff,
    gap: &'a GapInterval,
    layout: Layout,
    old: Option<&'a Arc<SourceDocument>>,
    new: Option<&'a Arc<SourceDocument>>,
}

fn append_expanded_row(rows: &mut Vec<PresentedRow>, expanded: ExpandedRow<'_>, offset: usize) {
    let ExpandedRow {
        file_index,
        file,
        gap,
        layout,
        old,
        new,
    } = expanded;
    let make = |side: DiffSide, range: &Range<usize>, source: Option<&Arc<SourceDocument>>| {
        let line_number = range.start.saturating_add(offset);
        if line_number >= range.end {
            return None;
        }
        let text = source?.line(line_number)?;
        Some(cell(
            None,
            Some(SourceLineRef { side, line_number }),
            text,
            DiffTone::Context,
        ))
    };
    let (left, right) = match layout {
        Layout::Split => (
            make(DiffSide::Old, &gap.old, old),
            make(DiffSide::New, &gap.new, new),
        ),
        Layout::Unified if file.status == FileStatus::Deleted => {
            (make(DiffSide::Old, &gap.old, old), None)
        }
        Layout::Unified => (None, make(DiffSide::New, &gap.new, new)),
    };
    if left.is_none() && right.is_none() {
        return;
    }
    let old_number = left.as_ref().and_then(PresentedCell::line_number);
    let new_number = right.as_ref().and_then(PresentedCell::line_number);
    rows.push(PresentedRow {
        id: row_id(&file.path, "expanded-context", None, old_number, new_number),
        kind: RowKind::ExpandedContext,
        file_index,
        hunk_index: None,
        left,
        right,
    });
}

fn placeholder_text(file: &FileDiff) -> Arc<str> {
    if let Some(bytes) = file.omitted_bytes {
        return format!("File content omitted ({bytes} bytes)").into();
    }
    if file.binary {
        "Binary file changed".into()
    } else if file.mode.is_some() {
        "File mode changed".into()
    } else {
        "Empty file changed".into()
    }
}

fn header_row(file_index: usize, file: &FileDiff) -> PresentedRow {
    let text = file
        .old_path
        .as_ref()
        .filter(|old| *old != &file.path)
        .map_or_else(
            || file.path.to_string(),
            |old| format!("{old} → {}", file.path),
        );
    PresentedRow {
        id: row_id(&file.path, "file", None, None, None),
        kind: RowKind::FileHeader,
        file_index,
        hunk_index: None,
        left: None,
        right: Some(cell(None, None, text, DiffTone::Meta)),
    }
}

fn hunk_header_row(
    file_index: usize,
    hunk_index: usize,
    hunk: &crate::Hunk,
    path: &RepoPath,
) -> PresentedRow {
    PresentedRow {
        id: row_id(path, "hunk", Some(hunk_index), None, None),
        kind: RowKind::HunkHeader,
        file_index,
        hunk_index: Some(hunk_index),
        left: None,
        right: Some(cell(None, None, hunk.header.as_str(), DiffTone::Meta)),
    }
}

fn append_unified_rows(
    rows: &mut Vec<PresentedRow>,
    file_index: usize,
    hunk_index: usize,
    file: &FileDiff,
) {
    for (line_index, line) in file.hunks[hunk_index].lines.iter().enumerate() {
        rows.push(unified_row(
            file_index, hunk_index, line_index, &file.path, line,
        ));
        append_no_newline_meta(rows, file_index, hunk_index, &file.path, line_index, line);
    }
}

fn unified_row(
    file_index: usize,
    hunk_index: usize,
    line_index: usize,
    path: &RepoPath,
    line: &PatchLine,
) -> PresentedRow {
    let side = match line.kind {
        PatchLineKind::Removed => DiffSide::Old,
        _ => DiffSide::New,
    };
    let presented = code_cell(side, hunk_index, line_index, line);
    let (left, right) = match side {
        DiffSide::Old => (Some(presented), None),
        DiffSide::New => (None, Some(presented)),
    };
    PresentedRow {
        id: code_row_id(path, hunk_index, left.as_ref(), right.as_ref()),
        kind: RowKind::Code,
        file_index,
        hunk_index: Some(hunk_index),
        left,
        right,
    }
}

fn append_split_rows(
    rows: &mut Vec<PresentedRow>,
    file_index: usize,
    hunk_index: usize,
    file: &FileDiff,
) {
    let lines = &file.hunks[hunk_index].lines;
    for group in split_groups(lines) {
        match group {
            SplitGroup::Single { line, index } => {
                let present = Some((index, line));
                let (left, right) = match line.kind {
                    PatchLineKind::Added => (None, present),
                    PatchLineKind::Removed => (present, None),
                    _ => (present, present),
                };
                rows.push(split_code_row(file_index, hunk_index, file, left, right));
                append_no_newline_meta(rows, file_index, hunk_index, &file.path, index, line);
            }
            SplitGroup::Changed { removed, added } => {
                for (left, right) in pair_changed_block(&removed, &added) {
                    rows.push(split_code_row(
                        file_index,
                        hunk_index,
                        file,
                        left.map(|side| (side.index, side.line)),
                        right.map(|side| (side.index, side.line)),
                    ));
                    if let Some(side) = left
                        .filter(|side| side.line.no_newline)
                        .or_else(|| right.filter(|side| side.line.no_newline))
                    {
                        append_no_newline_meta(
                            rows, file_index, hunk_index, &file.path, side.index, side.line,
                        );
                    }
                }
            }
        }
    }
}

fn append_no_newline_meta(
    rows: &mut Vec<PresentedRow>,
    file_index: usize,
    hunk_index: usize,
    path: &RepoPath,
    line_index: usize,
    line: &PatchLine,
) {
    if line.no_newline {
        rows.push(meta_row(
            file_index,
            Some(hunk_index),
            path,
            "no-newline",
            Some(line_index),
            NO_NEWLINE_TEXT,
        ));
    }
}

fn split_code_row(
    file_index: usize,
    hunk_index: usize,
    file: &FileDiff,
    left: Option<(usize, &PatchLine)>,
    right: Option<(usize, &PatchLine)>,
) -> PresentedRow {
    let build = |side: DiffSide, source: Option<(usize, &PatchLine)>| {
        let (index, line) = source?;
        line.line_number(side)?;
        Some(code_cell(side, hunk_index, index, line))
    };
    let left = build(DiffSide::Old, left);
    let right = build(DiffSide::New, right);
    PresentedRow {
        id: code_row_id(&file.path, hunk_index, left.as_ref(), right.as_ref()),
        kind: RowKind::Code,
        file_index,
        hunk_index: Some(hunk_index),
        left,
        right,
    }
}

fn cell(
    patch_source: Option<CellSource>,
    source_line: Option<SourceLineRef>,
    text: impl Into<Arc<str>>,
    tone: DiffTone,
) -> PresentedCell {
    PresentedCell {
        patch_source,
        source_line,
        text: text.into(),
        tone,
    }
}

fn code_cell(
    side: DiffSide,
    hunk_index: usize,
    line_index: usize,
    line: &PatchLine,
) -> PresentedCell {
    let line_number = (!matches!(line.kind, PatchLineKind::Meta | PatchLineKind::HunkHeader))
        .then(|| line.line_number(side))
        .flatten();
    let real_source = line_number.is_some();
    let source = real_source.then_some(CellSource {
        side,
        hunk_index,
        line_index,
    });
    cell(
        source,
        line_number.map(|line_number| SourceLineRef { side, line_number }),
        Arc::clone(&line.text),
        line.kind.tone(),
    )
}

fn meta_row(
    file_index: usize,
    hunk_index: Option<usize>,
    path: &RepoPath,
    kind: &str,
    line_index: Option<usize>,
    text: impl Into<Arc<str>>,
) -> PresentedRow {
    PresentedRow {
        id: row_id(path, kind, hunk_index, line_index, None),
        kind: RowKind::Meta,
        file_index,
        hunk_index,
        left: None,
        right: Some(cell(None, None, text, DiffTone::Meta)),
    }
}

fn index_field(value: Option<usize>) -> [u8; 9] {
    let mut field = [0_u8; 9];
    if let Some(index) = value {
        field[0] = 1;
        field[1..].copy_from_slice(&u64::try_from(index).unwrap_or(u64::MAX).to_le_bytes());
    }
    field
}

fn row_id(
    path: &RepoPath,
    kind: &str,
    hunk: Option<usize>,
    left: Option<usize>,
    right: Option<usize>,
) -> RowId {
    let hunk = index_field(hunk);
    let left = index_field(left);
    let right = index_field(right);
    RowId(
        Fingerprint::of([
            path.as_str().as_bytes(),
            kind.as_bytes(),
            hunk.as_slice(),
            left.as_slice(),
            right.as_slice(),
        ])
        .to_u64(),
    )
}

fn code_row_id(
    path: &RepoPath,
    hunk_index: usize,
    left: Option<&PresentedCell>,
    right: Option<&PresentedCell>,
) -> RowId {
    let line_index = |cell: Option<&PresentedCell>| {
        cell.and_then(|cell| cell.patch_source)
            .map(|source| source.line_index)
    };
    row_id(
        path,
        "code",
        Some(hunk_index),
        line_index(left),
        line_index(right),
    )
}

fn split_groups(lines: &[PatchLine]) -> Vec<SplitGroup<'_>> {
    let mut groups = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].kind != PatchLineKind::Removed {
            groups.push(SplitGroup::Single {
                line: &lines[index],
                index,
            });
            index += 1;
            continue;
        }
        let sides = |range: Range<usize>| {
            range
                .map(|index| SplitSide {
                    line: &lines[index],
                    index,
                })
                .collect::<Vec<_>>()
        };
        let removed_start = index;
        index += lines[index..]
            .iter()
            .take_while(|line| line.kind == PatchLineKind::Removed)
            .count();
        let added_start = index;
        index += lines[index..]
            .iter()
            .take_while(|line| line.kind == PatchLineKind::Added)
            .count();
        groups.push(SplitGroup::Changed {
            removed: sides(removed_start..added_start),
            added: sides(added_start..index),
        });
    }
    groups
}

enum SplitGroup<'a> {
    Single {
        line: &'a PatchLine,
        index: usize,
    },
    Changed {
        removed: Vec<SplitSide<'a>>,
        added: Vec<SplitSide<'a>>,
    },
}

#[derive(Clone, Copy)]
struct SplitSide<'a> {
    line: &'a PatchLine,
    index: usize,
}

type ChangedPair<'a> = (Option<&'a SplitSide<'a>>, Option<&'a SplitSide<'a>>);

fn pair_changed_block<'a>(
    removed: &'a [SplitSide<'a>],
    added: &'a [SplitSide<'a>],
) -> Vec<ChangedPair<'a>> {
    let old: Vec<&str> = removed.iter().map(|side| side.line.text.as_ref()).collect();
    let new: Vec<&str> = added.iter().map(|side| side.line.text.as_ref()).collect();
    let mut pairs = Vec::new();
    let mut align = |old_range: Range<usize>, new_range: Range<usize>| {
        let paired = old_range.len().min(new_range.len());
        for offset in 0..paired {
            pairs.push((
                Some(&removed[old_range.start + offset]),
                Some(&added[new_range.start + offset]),
            ));
        }
        pairs.extend(
            removed[old_range.start + paired..old_range.end]
                .iter()
                .map(|side| (Some(side), None)),
        );
        pairs.extend(
            added[new_range.start + paired..new_range.end]
                .iter()
                .map(|side| (None, Some(side))),
        );
    };
    for op in TextDiff::from_slices(&old, &new).ops() {
        match *op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => align(old_index..old_index + len, new_index..new_index + len),
            DiffOp::Delete {
                old_index, old_len, ..
            } => align(old_index..old_index + old_len, 0..0),
            DiffOp::Insert {
                new_index, new_len, ..
            } => align(0..0, new_index..new_index + new_len),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => align(
                old_index..old_index + old_len,
                new_index..new_index + new_len,
            ),
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileDiff, Hunk, ModeChange, SourceUnavailable, testing::DocumentBuilder};

    fn document() -> Arc<DiffDocument> {
        Arc::new(DiffDocument {
            repo_root: "/repo".into(),
            files: vec![
                FileDiff::from_texts(
                    "src/a.rs",
                    "same\none\nkeep\ntwo\n",
                    "same\nONE\nkeep\nTWO\nextra\n",
                )
                .unwrap(),
            ],
        })
    }

    fn presentation(view_mode: ViewMode) -> DiffPresentation {
        DiffPresentation::new(
            document(),
            PresentationOptions {
                view_mode,
                ..PresentationOptions::default()
            },
        )
    }

    #[test]
    fn hunk_sequences_expose_bounded_side_lines_for_patch_only_cells() {
        let presentation = presentation(ViewMode::Unified);
        let (row, cell) = (0..presentation.row_count())
            .find_map(|index| {
                let row = presentation.row(index)?;
                let cell = row.cell(DiffSide::New)?;
                (cell.text.as_ref() == "ONE").then_some((row, cell))
            })
            .unwrap();
        let sequence = presentation.hunk_sequence(row, cell).unwrap();
        let lines: Vec<&str> = sequence.lines().collect();
        assert_eq!(lines, ["same", "ONE", "keep", "TWO", "extra"]);
        assert_eq!(sequence.target_line, 1);
        assert_eq!(sequence.path, "src/a.rs");
        assert_eq!(
            sequence.id,
            SourceSequenceId::from_lines(lines.iter().copied())
        );

        let big = DocumentBuilder::new()
            .generated("src/big.rs", MAX_HUNK_SEQUENCE_LINES + 1)
            .build();
        let presentation = DiffPresentation::new(big, PresentationOptions::default());
        let row = (0..presentation.row_count())
            .filter_map(|index| presentation.row(index))
            .find(|row| row.kind == RowKind::Code)
            .unwrap();
        let cell = row.primary_cell().unwrap();
        assert!(
            presentation.hunk_sequence(row, cell).is_none(),
            "oversized hunks degrade to per-line highlighting"
        );
    }

    #[test]
    fn view_modes_resolve_and_cycle() {
        assert_eq!(ViewMode::Auto.resolve(true), Layout::Split);
        assert_eq!(ViewMode::Auto.resolve(false), Layout::Unified);
        assert_eq!(ViewMode::Unified.resolve(true), Layout::Unified);
        assert_eq!(ViewMode::Split.next().next(), ViewMode::Unified);
    }

    #[test]
    fn random_access_ranges_are_clamped() {
        let presentation = DiffPresentation::new(document(), PresentationOptions::default());
        assert_eq!(presentation.rows(usize::MAX..usize::MAX).len(), 0);
        assert_eq!(
            presentation.file_range(0).unwrap(),
            0..presentation.row_count()
        );
        assert!(presentation.hunk_range(0, 0).is_some());
        assert_eq!(presentation.language_at(1), "rs");
    }

    #[test]
    fn split_pairs_equal_lines_inside_changed_blocks() {
        let first = presentation(ViewMode::Split);
        let second = presentation(ViewMode::Split);
        let ids = |value: &DiffPresentation| {
            value
                .rows(0..value.row_count())
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&first), ids(&second));
        assert!(first.rows(0..first.row_count()).iter().any(|row| {
            row.left.as_ref().is_some_and(|c| c.text.as_ref() == "keep")
                && row
                    .right
                    .as_ref()
                    .is_some_and(|c| c.text.as_ref() == "keep")
        }));
    }

    #[test]
    fn unified_and_split_expose_all_code_anchors() {
        let unified = presentation(ViewMode::Unified);
        let split = presentation(ViewMode::Split);
        let commentable_cells = |value: &DiffPresentation| {
            value
                .rows(0..value.row_count())
                .iter()
                .flat_map(PresentedRow::sources)
                .count()
        };
        assert!(split.row_count() <= unified.row_count());
        assert!(commentable_cells(&split) >= commentable_cells(&unified));
    }

    #[test]
    fn anchors_are_resolved_on_demand_and_match_the_document() {
        let presentation = presentation(ViewMode::Unified);
        let index = presentation
            .file_range(0)
            .unwrap()
            .find(|index| presentation.is_commentable(*index))
            .unwrap();
        let row = presentation.row(index).unwrap();
        let cell = row.primary_cell().unwrap();
        let source = cell.patch_source.unwrap();
        let anchor = presentation.cell_anchor(row, cell).unwrap();
        assert_eq!(
            anchor,
            LineAnchor::for_line(
                &presentation.document().files[0],
                source.side,
                source.hunk_index,
                source.line_index
            )
            .unwrap()
        );
        assert_eq!(
            presentation.anchor_at(index, source.side),
            Some(anchor.clone())
        );
        assert!(presentation.row_shows_anchor(row, &anchor));
        assert_eq!(presentation.row_showing_anchor(&anchor), Some(index));
    }

    #[test]
    fn anchors_do_not_match_rows_from_other_files() {
        let document = Arc::new(DiffDocument {
            repo_root: "/repo".into(),
            files: vec![
                FileDiff::from_texts("a.rs", "old\n", "new\n").unwrap(),
                FileDiff::from_texts("b.rs", "old\n", "new\n").unwrap(),
            ],
        });
        let presentation = DiffPresentation::new(document, PresentationOptions::default());
        let first = presentation
            .file_range(0)
            .unwrap()
            .find(|index| presentation.is_commentable(*index))
            .unwrap();
        let second = presentation
            .file_range(1)
            .unwrap()
            .find(|index| presentation.is_commentable(*index))
            .unwrap();
        let anchor = presentation.anchor_at(first, DiffSide::New).unwrap();
        assert!(presentation.row_shows_anchor(presentation.row(first).unwrap(), &anchor));
        assert!(!presentation.row_shows_anchor(presentation.row(second).unwrap(), &anchor));
    }

    #[test]
    fn cells_are_addressable_by_side() {
        let split = presentation(ViewMode::Split);
        let row = split
            .rows(0..split.row_count())
            .iter()
            .find(|row| row.left.is_some() && row.right.is_some())
            .unwrap();
        assert_eq!(row.cell(DiffSide::New), row.right.as_ref());
        assert_eq!(row.preferred_cell(DiffSide::Old), row.left.as_ref());
        assert_eq!(row.cells().count(), 2);
        assert!(row.is_commentable());
    }

    #[test]
    fn gap_geometry_covers_leading_middle_and_trailing_intervals() {
        let path = RepoPath::new("a.rs").unwrap();
        let file = FileDiff {
            old_path: Some(path.clone()),
            path,
            status: FileStatus::Modified,
            staged: crate::StageState::Unstaged,
            hunks: vec![
                Hunk {
                    header: "@@ -3,2 +3,2 @@".into(),
                    function_context: None,
                    old_start: 3,
                    old_count: 2,
                    new_start: 3,
                    new_count: 2,
                    lines: Vec::new(),
                },
                Hunk {
                    header: "@@ -8 +8 @@".into(),
                    function_context: None,
                    old_start: 8,
                    old_count: 1,
                    new_start: 8,
                    new_count: 1,
                    lines: Vec::new(),
                },
            ],
            binary: false,
            mode: None,
            no_newline_at_end: false,
            omitted_bytes: None,
        };
        assert_eq!(
            gaps_for_file(&file, 10, 10),
            vec![
                GapInterval {
                    old: 1..3,
                    new: 1..3,
                },
                GapInterval {
                    old: 5..8,
                    new: 5..8,
                },
                GapInterval {
                    old: 9..11,
                    new: 9..11,
                },
            ]
        );
    }

    #[test]
    fn zero_count_hunks_use_the_next_real_line_as_the_gap_boundary() {
        let path = RepoPath::new("a.rs").unwrap();
        let file = FileDiff {
            old_path: Some(path.clone()),
            path,
            status: FileStatus::Modified,
            staged: crate::StageState::Unstaged,
            hunks: vec![Hunk {
                header: "@@ -2,0 +3 @@".into(),
                function_context: None,
                old_start: 2,
                old_count: 0,
                new_start: 3,
                new_count: 1,
                lines: Vec::new(),
            }],
            binary: false,
            mode: None,
            no_newline_at_end: false,
            omitted_bytes: None,
        };
        assert_eq!(
            gaps_for_file(&file, 5, 6),
            vec![
                GapInterval {
                    old: 1..3,
                    new: 1..3,
                },
                GapInterval {
                    old: 3..6,
                    new: 4..7,
                },
            ]
        );
    }

    #[test]
    fn empty_projection_is_row_for_row_identical_to_the_compatibility_path() {
        for view_mode in [ViewMode::Auto, ViewMode::Unified, ViewMode::Split] {
            let options = PresentationOptions {
                view_mode,
                ..PresentationOptions::default()
            };
            let expected = DiffPresentation::new(document(), options);
            let actual =
                DiffPresentation::with_sources(document(), options, &ContentProjection::default());
            assert_eq!(
                expected.rows(0..expected.row_count()),
                actual.rows(0..actual.row_count())
            );
        }
    }

    #[test]
    #[allow(clippy::format_collect)]
    fn split_full_file_projection_orders_both_sources_and_has_stable_row_ids() {
        let old = (1..=60)
            .map(|line| format!("line {line}\n"))
            .collect::<String>();
        let new = old.replace("line 31\n", "changed 31\n");
        let document = DocumentBuilder::new()
            .changed_with_hunk_window("a.rs", &old, &new, 28..=34)
            .build();
        let path = document.files[0].path.clone();
        let mut projection = ContentProjection::default();
        projection.insert_source(
            SourceKey {
                review_path: path.clone(),
                side: DiffSide::Old,
            },
            Arc::new(SourceDocument::try_from_text(&old).unwrap()),
        );
        projection.insert_source(
            SourceKey {
                review_path: path.clone(),
                side: DiffSide::New,
            },
            Arc::new(SourceDocument::try_from_text(&new).unwrap()),
        );
        projection.set_full_file(path, true);
        let options = PresentationOptions {
            view_mode: ViewMode::Split,
            ..PresentationOptions::default()
        };
        let first = DiffPresentation::with_sources(document.clone(), options, &projection);
        let second = DiffPresentation::with_sources(document, options, &projection);
        let source_lines = |presentation: &DiffPresentation, side| {
            presentation
                .rows(0..presentation.row_count())
                .iter()
                .filter_map(|row| row.cell(side)?.source_line)
                .map(|source| source.line_number)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            source_lines(&first, DiffSide::Old),
            (1..=60).collect::<Vec<_>>()
        );
        assert_eq!(
            source_lines(&first, DiffSide::New),
            (1..=60).collect::<Vec<_>>()
        );
        assert_eq!(
            first
                .rows(0..first.row_count())
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>(),
            second
                .rows(0..second.row_count())
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>()
        );
        assert!(first.rows(0..first.row_count()).iter().any(|row| {
            row.kind == RowKind::ExpandedContext
                && !row.is_commentable()
                && row.left.is_some()
                && row.right.is_some()
        }));
    }

    #[test]
    fn mode_only_full_file_intent_projects_unavailable_and_complete_source() {
        let path = RepoPath::new("script.sh").unwrap();
        let file = FileDiff {
            old_path: Some(path.clone()),
            path: path.clone(),
            status: FileStatus::Modified,
            staged: crate::StageState::Unstaged,
            hunks: Vec::new(),
            binary: false,
            mode: Some(ModeChange {
                old: Some("100644".into()),
                new: Some("100755".into()),
            }),
            no_newline_at_end: false,
            omitted_bytes: None,
        };
        let document = Arc::new(DiffDocument {
            repo_root: "/repo".into(),
            files: vec![file],
        });
        let key = SourceKey {
            review_path: path.clone(),
            side: DiffSide::New,
        };
        let mut projection = ContentProjection::default();
        projection.set_full_file(path, true);
        let absent = DiffPresentation::with_sources(
            document.clone(),
            PresentationOptions::default(),
            &projection,
        );
        let absent_gap = absent
            .rows(0..absent.row_count())
            .iter()
            .position(|row| row.kind == RowKind::ExpandGap)
            .unwrap();
        assert_eq!(
            absent.gap_info(absent_gap).unwrap().unavailable,
            Some(SourceUnavailable::Absent)
        );

        projection.insert_unavailable(key.clone(), SourceUnavailable::Binary);
        let unavailable = DiffPresentation::with_sources(
            document.clone(),
            PresentationOptions::default(),
            &projection,
        );
        let gap = unavailable
            .rows(0..unavailable.row_count())
            .iter()
            .position(|row| row.kind == RowKind::ExpandGap)
            .unwrap();
        assert_eq!(
            unavailable.gap_info(gap).unwrap().unavailable,
            Some(SourceUnavailable::Binary)
        );

        projection.insert_source(
            key,
            Arc::new(SourceDocument::try_from_text("#!/bin/sh\necho ok\n").unwrap()),
        );
        let loaded =
            DiffPresentation::with_sources(document, PresentationOptions::default(), &projection);
        assert_eq!(
            loaded
                .rows(0..loaded.row_count())
                .iter()
                .filter(|row| row.kind == RowKind::ExpandedContext)
                .count(),
            2
        );
    }

    #[test]
    fn presentation_shares_document_text_instead_of_copying_it() {
        let document = document();
        let presentation = DiffPresentation::new(document.clone(), PresentationOptions::default());
        let line = &document.files[0].hunks[0].lines[0];
        let cell = presentation
            .rows(0..presentation.row_count())
            .iter()
            .find_map(|row| row.primary_cell().filter(|c| c.text == line.text))
            .unwrap();
        assert!(Arc::ptr_eq(&cell.text, &line.text));
    }
}
