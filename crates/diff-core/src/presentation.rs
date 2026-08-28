//! Framework-neutral, random-access unified and split row presentation.

use crate::{DiffDocument, DiffSide, FileDiff, LineAnchor, PatchLine, PatchLineKind, RepoPath};
use serde::{Deserialize, Serialize};
use similar::{DiffOp, TextDiff};
use std::{ops::Range, sync::Arc};

/// Requested diff layout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    /// Let an adapter choose based on its own width units.
    #[default]
    Auto,
    /// One code column.
    Unified,
    /// Old and new columns aligned side by side.
    Split,
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
    pub const fn resolved_mode(self) -> ViewMode {
        match (self.view_mode, self.split_when_auto) {
            (ViewMode::Auto, true) => ViewMode::Split,
            (ViewMode::Auto, false) => ViewMode::Unified,
            (mode, _) => mode,
        }
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

/// Semantic tint for a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DiffTone {
    Context,
    Added,
    Removed,
    Meta,
}

/// Stable identity for a presented row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RowId(pub u64);

/// One side of a presented row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentedCell {
    pub anchor: Option<LineAnchor>,
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

/// Eager row indexes and cheap descriptors. Syntax and frontend widgets are not
/// constructed here.
#[derive(Debug, Clone)]
pub struct DiffPresentation {
    document: Arc<DiffDocument>,
    mode: ViewMode,
    rows: Vec<PresentedRow>,
    file_ranges: Vec<Range<usize>>,
    hunk_ranges: Vec<Vec<Range<usize>>>,
}

impl DiffPresentation {
    /// Indexes a document once for O(1) row lookup and slicing.
    pub fn new(document: Arc<DiffDocument>, options: PresentationOptions) -> Self {
        let mode = options.resolved_mode();
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
                rows.push(hunk_header_row(
                    file_index,
                    hunk_index,
                    &hunk.header,
                    &file.path,
                ));
                match mode {
                    ViewMode::Unified | ViewMode::Auto => {
                        for (line_index, line) in hunk.lines.iter().enumerate() {
                            rows.push(unified_row(file_index, hunk_index, line_index, file, line));
                            if line.no_newline {
                                rows.push(meta_row(
                                    file_index,
                                    Some(hunk_index),
                                    &file.path,
                                    format!("no-newline:{line_index}"),
                                    "\\ No newline at end of file",
                                ));
                            }
                        }
                    }
                    ViewMode::Split => append_split_rows(&mut rows, file_index, hunk_index, file),
                }
                file_hunks.push(hunk_start..rows.len());
            }
            if file.hunks.is_empty() {
                let text = if file.binary {
                    "Binary file changed"
                } else if file.mode.is_some() {
                    "File mode changed"
                } else {
                    "Empty file changed"
                };
                rows.push(meta_row(file_index, None, &file.path, "placeholder", text));
            }
            file_ranges.push(file_start..rows.len());
            hunk_ranges.push(file_hunks);
        }
        Self {
            document,
            mode,
            rows,
            file_ranges,
            hunk_ranges,
        }
    }

    /// Returns the retained immutable snapshot.
    pub fn document(&self) -> &Arc<DiffDocument> {
        &self.document
    }

    /// Returns the concrete indexed mode.
    pub const fn view_mode(&self) -> ViewMode {
        self.mode
    }

    /// Total number of rows.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Returns one row in O(1).
    pub fn row(&self, index: usize) -> Option<&PresentedRow> {
        self.rows.get(index)
    }

    /// Returns a clamped visible row slice without allocating.
    pub fn rows(&self, range: Range<usize>) -> &[PresentedRow] {
        let start = range.start.min(self.rows.len());
        let end = range.end.max(start).min(self.rows.len());
        &self.rows[start..end]
    }

    /// Returns the row range occupied by a file.
    pub fn file_range(&self, file_index: usize) -> Option<Range<usize>> {
        self.file_ranges.get(file_index).cloned()
    }

    /// Returns the row range occupied by a hunk, including its header.
    pub fn hunk_range(&self, file_index: usize, hunk_index: usize) -> Option<Range<usize>> {
        self.hunk_ranges.get(file_index)?.get(hunk_index).cloned()
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
        id: row_id(&file.path, "file", None, None),
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
    text: &str,
    path: &RepoPath,
) -> PresentedRow {
    PresentedRow {
        id: row_id(path, "hunk", Some(hunk_index), None),
        kind: RowKind::HunkHeader,
        file_index,
        hunk_index: Some(hunk_index),
        left: None,
        right: Some(cell(None, None, text, DiffTone::Meta)),
    }
}

fn unified_row(
    file_index: usize,
    hunk_index: usize,
    line_index: usize,
    file: &FileDiff,
    line: &PatchLine,
) -> PresentedRow {
    let (side, tone) = match line.kind {
        PatchLineKind::Removed => (DiffSide::Old, DiffTone::Removed),
        PatchLineKind::Added => (DiffSide::New, DiffTone::Added),
        _ => (DiffSide::New, DiffTone::Context),
    };
    let anchor = LineAnchor::for_line(file, side, hunk_index, line_index);
    let presented = cell(
        anchor.clone(),
        line.line_number(side),
        line.text.as_str(),
        tone,
    );
    PresentedRow {
        id: anchor.as_ref().map_or_else(
            || row_id(&file.path, "code", Some(hunk_index), Some(line_index)),
            anchor_row_id,
        ),
        kind: RowKind::Code,
        file_index,
        hunk_index: Some(hunk_index),
        left: (side == DiffSide::Old).then_some(presented.clone()),
        right: (side == DiffSide::New).then_some(presented),
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
                if line.kind == PatchLineKind::Context {
                    rows.push(split_code_row(
                        file_index,
                        hunk_index,
                        file,
                        Some((index, line)),
                        Some((index, line)),
                    ));
                } else if line.kind == PatchLineKind::Added {
                    rows.push(split_code_row(
                        file_index,
                        hunk_index,
                        file,
                        None,
                        Some((index, line)),
                    ));
                } else {
                    rows.push(split_code_row(
                        file_index,
                        hunk_index,
                        file,
                        Some((index, line)),
                        None,
                    ));
                }
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
            format!("no-newline:{line_index}"),
            "\\ No newline at end of file",
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
    let left = left.and_then(|(index, line)| {
        let anchor = LineAnchor::for_line(file, DiffSide::Old, hunk_index, index)?;
        Some(cell(
            Some(anchor),
            line.old_line_no,
            line.text.as_str(),
            tone(line.kind),
        ))
    });
    let right = right.and_then(|(index, line)| {
        let anchor = LineAnchor::for_line(file, DiffSide::New, hunk_index, index)?;
        Some(cell(
            Some(anchor),
            line.new_line_no,
            line.text.as_str(),
            tone(line.kind),
        ))
    });
    let id = match (
        left.as_ref().and_then(|value| value.anchor.as_ref()),
        right.as_ref().and_then(|value| value.anchor.as_ref()),
    ) {
        (Some(left), Some(right)) => paired_row_id(left, right),
        (Some(anchor), None) | (None, Some(anchor)) => anchor_row_id(anchor),
        (None, None) => row_id(&file.path, "empty-code", Some(hunk_index), None),
    };
    PresentedRow {
        id,
        kind: RowKind::Code,
        file_index,
        hunk_index: Some(hunk_index),
        left,
        right,
    }
}

fn cell(
    anchor: Option<LineAnchor>,
    line_number: Option<usize>,
    text: impl Into<Arc<str>>,
    tone: DiffTone,
) -> PresentedCell {
    PresentedCell {
        anchor,
        line_number,
        text: text.into(),
        tone,
    }
}

fn meta_row(
    file_index: usize,
    hunk_index: Option<usize>,
    path: &RepoPath,
    discriminator: impl AsRef<str>,
    text: impl Into<Arc<str>>,
) -> PresentedRow {
    PresentedRow {
        id: row_id(path, discriminator.as_ref(), hunk_index, None),
        kind: RowKind::Meta,
        file_index,
        hunk_index,
        left: None,
        right: Some(cell(None, None, text, DiffTone::Meta)),
    }
}

fn tone(kind: PatchLineKind) -> DiffTone {
    match kind {
        PatchLineKind::Added => DiffTone::Added,
        PatchLineKind::Removed => DiffTone::Removed,
        _ => DiffTone::Context,
    }
}

fn anchor_row_id(anchor: &LineAnchor) -> RowId {
    let old = anchor
        .old_line_no
        .map(|value| value.to_string())
        .unwrap_or_default();
    let new = anchor
        .new_line_no
        .map(|value| value.to_string())
        .unwrap_or_default();
    hash_id([
        anchor.path.as_str().as_bytes(),
        match anchor.side {
            DiffSide::Old => b"old",
            DiffSide::New => b"new",
        },
        old.as_bytes(),
        new.as_bytes(),
        anchor.fingerprint.as_bytes(),
    ])
}

fn paired_row_id(left: &LineAnchor, right: &LineAnchor) -> RowId {
    let left_id = anchor_row_id(left).0.to_le_bytes();
    let right_id = anchor_row_id(right).0.to_le_bytes();
    hash_id([&left_id, b"pair", &right_id])
}

fn row_id(path: &RepoPath, kind: &str, hunk: Option<usize>, line: Option<usize>) -> RowId {
    let hunk = hunk.map(|value| value.to_string()).unwrap_or_default();
    let line = line.map(|value| value.to_string()).unwrap_or_default();
    hash_id([
        path.as_str().as_bytes(),
        kind.as_bytes(),
        hunk.as_bytes(),
        line.as_bytes(),
    ])
}

fn hash_id<const N: usize>(parts: [&[u8]; N]) -> RowId {
    let mut hasher = blake3::Hasher::new();
    for part in parts {
        hasher.update(part);
        hasher.update(&[0]);
    }
    let bytes: [u8; 8] = hasher.finalize().as_bytes()[..8]
        .try_into()
        .expect("BLAKE3 digest has eight bytes");
    RowId(u64::from_le_bytes(bytes))
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

struct SplitSide<'a> {
    line: &'a PatchLine,
    index: usize,
}

fn pair_changed_block<'a>(
    removed: &'a [SplitSide<'a>],
    added: &'a [SplitSide<'a>],
) -> Vec<(Option<&'a SplitSide<'a>>, Option<&'a SplitSide<'a>>)> {
    let old: Vec<&str> = removed.iter().map(|side| side.line.text.as_str()).collect();
    let new: Vec<&str> = added.iter().map(|side| side.line.text.as_str()).collect();
    let diff = TextDiff::from_slices(&old, &new);
    let mut pairs = Vec::new();
    for op in diff.ops() {
        match *op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                for offset in 0..len {
                    pairs.push((
                        Some(&removed[old_index + offset]),
                        Some(&added[new_index + offset]),
                    ));
                }
            }
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                pairs.extend(
                    removed[old_index..old_index + old_len]
                        .iter()
                        .map(|side| (Some(side), None)),
                );
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                pairs.extend(
                    added[new_index..new_index + new_len]
                        .iter()
                        .map(|side| (None, Some(side))),
                );
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                let paired = old_len.min(new_len);
                for offset in 0..paired {
                    pairs.push((
                        Some(&removed[old_index + offset]),
                        Some(&added[new_index + offset]),
                    ));
                }
                pairs.extend(
                    removed[old_index + paired..old_index + old_len]
                        .iter()
                        .map(|side| (Some(side), None)),
                );
                pairs.extend(
                    added[new_index + paired..new_index + new_len]
                        .iter()
                        .map(|side| (None, Some(side))),
                );
            }
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

    #[test]
    fn random_access_ranges_are_clamped() {
        let presentation = DiffPresentation::new(document(), PresentationOptions::default());
        assert_eq!(presentation.rows(usize::MAX..usize::MAX).len(), 0);
        assert_eq!(
            presentation.file_range(0).unwrap(),
            0..presentation.row_count()
        );
        assert!(presentation.hunk_range(0, 0).is_some());
    }

    #[test]
    fn split_pairs_equal_lines_inside_changed_blocks() {
        let options = PresentationOptions {
            view_mode: ViewMode::Split,
            ..PresentationOptions::default()
        };
        let first = DiffPresentation::new(document(), options);
        let second = DiffPresentation::new(document(), options);
        assert_eq!(first.row_count(), second.row_count());
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
            row.left
                .as_ref()
                .is_some_and(|cell| cell.text.as_ref() == "keep")
                && row
                    .right
                    .as_ref()
                    .is_some_and(|cell| cell.text.as_ref() == "keep")
        }));
    }

    #[test]
    fn unified_and_split_expose_all_code_anchors() {
        let unified = DiffPresentation::new(
            document(),
            PresentationOptions {
                view_mode: ViewMode::Unified,
                ..PresentationOptions::default()
            },
        );
        let split = DiffPresentation::new(
            document(),
            PresentationOptions {
                view_mode: ViewMode::Split,
                ..PresentationOptions::default()
            },
        );
        let anchor_count = |presentation: &DiffPresentation| {
            presentation
                .rows(0..presentation.row_count())
                .iter()
                .flat_map(|row| [row.left.as_ref(), row.right.as_ref()])
                .flatten()
                .filter(|cell| cell.anchor.is_some())
                .count()
        };
        assert!(split.row_count() <= unified.row_count());
        assert!(anchor_count(&split) >= anchor_count(&unified));
    }
}
