//! Framework-neutral, random-access unified and split row presentation.

use crate::{
    DiffDocument, DiffSide, FileDiff, Fingerprint, LineAnchor, PatchLine, PatchLineKind, RepoPath,
};
use serde::{Deserialize, Serialize};
use similar::{DiffOp, TextDiff};
use std::{
    collections::HashMap,
    ops::Range,
    sync::{Arc, OnceLock},
};

const NO_NEWLINE_TEXT: &str = "\\ No newline at end of file";

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
}

pub use diff_theme::DiffTone;

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
    pub source: Option<CellSource>,
    pub line_number: Option<usize>,
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

    pub fn sources(&self) -> impl Iterator<Item = CellSource> {
        self.cells().filter_map(|cell| cell.source)
    }

    #[must_use]
    pub fn is_commentable(&self) -> bool {
        self.kind == RowKind::Code && self.sources().next().is_some()
    }
}

/// Content-derived identity of one ordered source sequence.
///
/// Equal sequences intentionally share an identity. The fingerprint scheme is an
/// implementation detail and sequence IDs should not be persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SequenceId(Fingerprint);

impl From<SequenceId> for Fingerprint {
    fn from(id: SequenceId) -> Self {
        id.0
    }
}

/// Renderer-neutral description of the source sequence containing a diff cell.
///
/// Renderers pass these lines to their syntax engine so multiline constructs are
/// interpreted with bounded preceding context without coupling `diff-core` to a
/// particular highlighter.
pub struct CellSequence<'a> {
    pub id: SequenceId,
    pub language: &'a str,
    pub target_line: usize,
    pub lines: Box<dyn Iterator<Item = &'a str> + 'a>,
}

/// Eager row indexes and cheap descriptors. Syntax and frontend widgets are not
/// constructed here.
#[derive(Debug, Clone)]
pub struct DiffPresentation {
    document: Arc<DiffDocument>,
    layout: Layout,
    rows: Vec<PresentedRow>,
    anchor_rows: HashMap<(usize, DiffSide, usize), usize>,
    file_ranges: Vec<Range<usize>>,
    hunk_ranges: Vec<Vec<Range<usize>>>,
    /// Lazily fingerprinted per hunk so presentations that never request
    /// highlights skip hashing the document.
    sequence_ids: Vec<Vec<OnceLock<[SequenceId; 2]>>>,
}

impl DiffPresentation {
    /// Indexes a document once for O(1) row lookup and slicing.
    #[must_use]
    pub fn new(document: Arc<DiffDocument>, options: PresentationOptions) -> Self {
        let layout = options.layout();
        let mut rows = Vec::new();
        let mut file_ranges = Vec::with_capacity(document.files.len());
        let mut hunk_ranges = Vec::with_capacity(document.files.len());
        for (file_index, file) in document.files.iter().enumerate() {
            let file_start = rows.len();
            if options.include_file_headers {
                rows.push(header_row(file_index, file));
            }
            let mut file_hunks = Vec::with_capacity(file.hunks.len());
            for (hunk_index, hunk) in file.hunks.iter().enumerate() {
                let hunk_start = rows.len();
                rows.push(hunk_header_row(file_index, hunk_index, hunk, &file.path));
                match layout {
                    Layout::Unified => {
                        append_unified_rows(&mut rows, file_index, hunk_index, file);
                    }
                    Layout::Split => {
                        append_split_rows(&mut rows, file_index, hunk_index, file);
                    }
                }
                file_hunks.push(hunk_start..rows.len());
            }
            if file.hunks.is_empty() {
                rows.push(meta_row(
                    file_index,
                    None,
                    &file.path,
                    "placeholder",
                    None,
                    placeholder_text(file),
                ));
            }
            file_ranges.push(file_start..rows.len());
            hunk_ranges.push(file_hunks);
        }
        let sequence_ids = document
            .files
            .iter()
            .map(|file| file.hunks.iter().map(|_| OnceLock::new()).collect())
            .collect();
        let anchor_rows = rows
            .iter()
            .enumerate()
            .flat_map(|(row_index, row)| {
                row.cells().filter_map(move |cell| {
                    Some((
                        (row.file_index, cell.source?.side, cell.line_number?),
                        row_index,
                    ))
                })
            })
            .collect();
        Self {
            document,
            layout,
            rows,
            anchor_rows,
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
        let source = cell.source?;
        let file = self.document.files.get(row.file_index)?;
        LineAnchor::for_line(file, source.side, source.hunk_index, source.line_index)
    }

    #[must_use]
    pub fn anchor_at(&self, row_index: usize, side: DiffSide) -> Option<LineAnchor> {
        let row = self.row(row_index)?;
        self.cell_anchor(row, row.preferred_cell(side)?)
    }

    /// Describes the complete source-side sequence containing `cell`.
    ///
    /// Returns `None` for synthetic cells and stale descriptors. The sequence
    /// identity is content-derived and must not be persisted.
    #[must_use]
    pub fn cell_sequence<'a>(
        &'a self,
        row: &PresentedRow,
        cell: &PresentedCell,
    ) -> Option<CellSequence<'a>> {
        let source = cell.source?;
        let file = self.document.files.get(row.file_index)?;
        let hunk = file.hunks.get(source.hunk_index)?;
        hunk.lines.get(source.line_index)?;
        let target = hunk.lines[..=source.line_index]
            .iter()
            .filter(|line| line.line_number(source.side).is_some())
            .count()
            .saturating_sub(1);
        let id = self.sequence_id(row.file_index, source.hunk_index, source.side)?;
        Some(CellSequence {
            id,
            language: file.path.as_str(),
            target_line: target,
            lines: Box::new(
                hunk.lines
                    .iter()
                    .filter(move |line| line.line_number(source.side).is_some())
                    .map(|line| line.text.as_ref()),
            ),
        })
    }

    fn sequence_id(&self, file_index: usize, hunk_index: usize, side: DiffSide) -> Option<SequenceId> {
        let hunk = self.document.files.get(file_index)?.hunks.get(hunk_index)?;
        let ids = self
            .sequence_ids
            .get(file_index)?
            .get(hunk_index)?
            .get_or_init(|| {
                [
                    source_sequence_id(&hunk.lines, DiffSide::Old),
                    source_sequence_id(&hunk.lines, DiffSide::New),
                ]
            });
        Some(match side {
            DiffSide::Old => ids[0],
            DiffSide::New => ids[1],
        })
    }

    #[must_use]
    pub fn language_at(&self, row_index: usize) -> &str {
        self.row(row_index)
            .map_or("", |row| self.language_at_row(row))
    }

    /// Repository path of the file a row belongs to, usable as a language hint
    /// when a cell has no [`CellSequence`].
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
    pub fn row_shows_anchor(&self, row: &PresentedRow, anchor: &LineAnchor) -> bool {
        self.document
            .files
            .get(row.file_index)
            .is_some_and(|file| file.path == anchor.path)
            && row.cells().any(|cell| {
                cell.source.is_some_and(|source| source.side == anchor.side)
                    && cell.line_number == anchor.line_number()
            })
    }

    #[must_use]
    pub fn is_commentable(&self, index: usize) -> bool {
        self.row(index).is_some_and(PresentedRow::is_commentable)
    }

    #[must_use]
    pub fn first_commentable(&self, range: Range<usize>) -> Option<usize> {
        range.into_iter().find(|index| self.is_commentable(*index))
    }

    #[must_use]
    pub fn last_commentable(&self, range: Range<usize>) -> Option<usize> {
        range.rev().find(|index| self.is_commentable(*index))
    }

    #[must_use]
    pub fn step_commentable(
        &self,
        from: usize,
        backward: bool,
        range: &Range<usize>,
    ) -> Option<usize> {
        if backward {
            self.last_commentable(range.start..from.min(range.end))
        } else {
            self.first_commentable(from.saturating_add(1).max(range.start)..range.end)
        }
    }
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
    source: Option<CellSource>,
    line_number: Option<usize>,
    text: impl Into<Arc<str>>,
    tone: DiffTone,
) -> PresentedCell {
    PresentedCell {
        source,
        line_number,
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
    let source = line.line_number(side).is_some().then_some(CellSource {
        side,
        hunk_index,
        line_index,
    });
    cell(
        source,
        line.line_number(side),
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

fn source_sequence_id(lines: &[PatchLine], side: DiffSide) -> SequenceId {
    let fields = std::iter::once(b"diff-source-sequence-v1".as_slice()).chain(
        lines
            .iter()
            .filter(|line| line.line_number(side).is_some())
            .map(|line| line.text.as_bytes()),
    );
    SequenceId(Fingerprint::of(fields))
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
        cell.and_then(|cell| cell.source)
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
    use crate::FileDiff;

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
            .first_commentable(presentation.file_range(0).unwrap())
            .unwrap();
        let row = presentation.row(index).unwrap();
        let cell = row.primary_cell().unwrap();
        let source = cell.source.unwrap();
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
    fn stepping_skips_non_source_rows_and_stops_at_boundaries() {
        let unified = presentation(ViewMode::Unified);
        let range = unified.file_range(0).unwrap();
        let first = unified.first_commentable(range.clone()).unwrap();
        let last = unified.last_commentable(range.clone()).unwrap();
        assert!(unified.step_commentable(first, true, &range).is_none());
        assert!(unified.step_commentable(last, false, &range).is_none());
        let second = unified.step_commentable(first, false, &range).unwrap();
        assert_eq!(unified.step_commentable(second, true, &range), Some(first));
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
            .first_commentable(presentation.file_range(0).unwrap())
            .unwrap();
        let second = presentation
            .first_commentable(presentation.file_range(1).unwrap())
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
    fn cell_sequence_preserves_multiline_source_context() {
        let document = Arc::new(DiffDocument {
            repo_root: "/repo".into(),
            files: vec![FileDiff::from_texts("a.rs", "", "/*\n comment */\n").unwrap()],
        });
        let presentation = DiffPresentation::new(document, PresentationOptions::default());
        let row = presentation
            .rows(0..presentation.row_count())
            .iter()
            .find(|row| {
                row.primary_cell()
                    .is_some_and(|cell| cell.text.as_ref() == " comment */")
            })
            .unwrap();
        let sequence = presentation
            .cell_sequence(row, row.primary_cell().unwrap())
            .unwrap();
        assert_eq!(sequence.language, "a.rs");
        assert_eq!(sequence.target_line, 1);
        assert_eq!(sequence.lines.collect::<Vec<_>>(), ["/*", " comment */"]);
    }

    #[test]
    fn sequence_identity_is_derived_from_visible_source_content() {
        let sequence_id = |text: &str| {
            let document = Arc::new(DiffDocument {
                repo_root: "/repo".into(),
                files: vec![FileDiff::from_texts("a.rs", "", text).unwrap()],
            });
            let presentation = DiffPresentation::new(document, PresentationOptions::default());
            let row = presentation
                .rows(0..presentation.row_count())
                .iter()
                .find(|row| row.primary_cell().is_some_and(|cell| cell.source.is_some()))
                .unwrap();
            presentation
                .cell_sequence(row, row.primary_cell().unwrap())
                .unwrap()
                .id
        };

        assert_eq!(sequence_id("let x = 1;\n"), sequence_id("let x = 1;\n"));
        assert_ne!(sequence_id("let x = 1;\n"), sequence_id("let x = 2;\n"));
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
