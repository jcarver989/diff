//! Normalization adapters for Git patches and porcelain status output.

use crate::{
    DiffDocument, DiffError, DiffScope, FileDiff, FileStatus, Hunk, ModeChange, PatchLine,
    PatchLineKind, RepoPath, StageState,
};
use diffy::{
    Line, Patch,
    patch_set::{FileMode, FileOperation, FilePatch, ParseOptions, PatchKind, PatchSet},
};
use std::collections::HashMap;

const MAX_TRACKED_PATCH_BYTES: u64 = 8 * 1024 * 1024;
const MAX_TRACKED_PATCH_LINES: usize = 20_000;

/// One entry from `git status --porcelain=v1 -z`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatusEntry {
    /// Current repository-relative path.
    pub path: RepoPath,
    /// Previous path for a rename or copy.
    pub old_path: Option<RepoPath>,
    /// Status represented by the most relevant porcelain column.
    pub status: FileStatus,
    /// Whether the change is in the index, worktree, or both.
    pub staged: StageState,
    /// Raw index status character (`X`).
    pub index: Option<char>,
    /// Raw worktree status character (`Y`).
    pub worktree: Option<char>,
}

/// Contents for an untracked file, supplied by a native Git host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntrackedFile {
    /// Repository-relative path.
    pub path: RepoPath,
    pub contents: Vec<u8>,
    pub omitted_bytes: Option<u64>,
}

/// Parses and normalizes a multi-file Git diff.
///
/// # Errors
/// Returns an error when the patch is malformed, contains unsupported path
/// encoding, or contains an invalid repository-relative path.
pub fn parse_git_diff(bytes: &[u8]) -> Result<Vec<FileDiff>, DiffError> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Vec::new());
    }
    PatchSet::parse_bytes(bytes, ParseOptions::gitdiff())
        .map(|patch| normalize_patch(&patch.map_err(|source| DiffError::Parse { source })?))
        .collect()
}

fn normalize_patch(patch: &FilePatch<'_, [u8]>) -> Result<FileDiff, DiffError> {
    let (previous, current, status) = normalize_operation(patch.operation())?;
    let (hunks, omitted_bytes) = match patch.patch() {
        PatchKind::Text(text) => {
            let (lines, bytes) = text_patch_size(text);
            if lines > MAX_TRACKED_PATCH_LINES || bytes > MAX_TRACKED_PATCH_BYTES {
                (Vec::new(), Some(bytes))
            } else {
                (normalize_hunks(text)?, None)
            }
        }
        PatchKind::Binary(_) => (Vec::new(), None),
    };
    let no_newline_at_end = hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .any(|line| line.no_newline);
    Ok(FileDiff {
        old_path: previous,
        path: current,
        status,
        staged: StageState::Unstaged,
        hunks,
        binary: patch.patch().is_binary(),
        mode: normalize_mode(patch),
        no_newline_at_end,
        omitted_bytes,
    })
}

fn text_patch_size(text: &Patch<'_, [u8]>) -> (usize, u64) {
    text.hunks().iter().flat_map(diffy::Hunk::lines).fold(
        (0_usize, 0_u64),
        |(lines, bytes), line| {
            let bytes_in_line = match line {
                Line::Context(bytes) | Line::Delete(bytes) | Line::Insert(bytes) => bytes.len(),
            };
            (
                lines.saturating_add(1),
                bytes.saturating_add(u64::try_from(bytes_in_line).unwrap_or(u64::MAX)),
            )
        },
    )
}

fn normalize_operation(
    operation: &FileOperation<'_, [u8]>,
) -> Result<(Option<RepoPath>, RepoPath, FileStatus), DiffError> {
    Ok(match operation {
        FileOperation::Create(raw) => (None, decode_path(raw, Some(b"b/"))?, FileStatus::Added),
        FileOperation::Delete(raw) => {
            let path = decode_path(raw, Some(b"a/"))?;
            (Some(path.clone()), path, FileStatus::Deleted)
        }
        FileOperation::Modify { original, modified } => {
            let before = decode_path(original, Some(b"a/"))?;
            let after = decode_path(modified, Some(b"b/"))?;
            let status = if before == after {
                FileStatus::Modified
            } else {
                FileStatus::Renamed
            };
            (Some(before), after, status)
        }
        FileOperation::Rename { from, to } => (
            Some(decode_path(from, None)?),
            decode_path(to, None)?,
            FileStatus::Renamed,
        ),
        FileOperation::Copy { from, to } => (
            Some(decode_path(from, None)?),
            decode_path(to, None)?,
            FileStatus::Copied,
        ),
    })
}

fn normalize_hunks(text: &Patch<'_, [u8]>) -> Result<Vec<Hunk>, DiffError> {
    text.hunks()
        .iter()
        .map(|hunk| {
            let old = hunk.old_range();
            let new = hunk.new_range();
            let mut old_line = old.start();
            let mut new_line = new.start();
            let lines = hunk
                .lines()
                .iter()
                .map(|line| {
                    let (kind, bytes) = match line {
                        Line::Context(bytes) => (PatchLineKind::Context, *bytes),
                        Line::Delete(bytes) => (PatchLineKind::Removed, *bytes),
                        Line::Insert(bytes) => (PatchLineKind::Added, *bytes),
                    };
                    let old_line_no = (kind != PatchLineKind::Added).then(|| {
                        let number = old_line;
                        old_line += 1;
                        number
                    });
                    let new_line_no = (kind != PatchLineKind::Removed).then(|| {
                        let number = new_line;
                        new_line += 1;
                        number
                    });
                    let no_newline = !bytes.ends_with(b"\n");
                    let text = std::str::from_utf8(bytes.strip_suffix(b"\n").unwrap_or(bytes))?;
                    Ok(PatchLine {
                        kind,
                        text: text.into(),
                        old_line_no,
                        new_line_no,
                        no_newline,
                    })
                })
                .collect::<Result<Vec<_>, DiffError>>()?;
            let function_context = hunk
                .function_context()
                .map(|bytes| {
                    std::str::from_utf8(bytes)
                        .map(|context| context.trim_end_matches(['\r', '\n']).to_owned())
                })
                .transpose()?;
            let suffix = function_context
                .as_deref()
                .map_or_else(String::new, |context| format!(" {context}"));
            Ok(Hunk {
                header: format!("@@ -{old} +{new} @@{suffix}"),
                function_context,
                old_start: old.start(),
                old_count: old.len(),
                new_start: new.start(),
                new_count: new.len(),
                lines,
            })
        })
        .collect()
}

fn normalize_mode(patch: &FilePatch<'_, [u8]>) -> Option<ModeChange> {
    let old = patch.old_mode().copied().map(mode_string);
    let new = patch.new_mode().copied().map(mode_string);
    (old != new && (old.is_some() || new.is_some())).then_some(ModeChange { old, new })
}

fn decode_path(bytes: &[u8], expected_prefix: Option<&[u8]>) -> Result<RepoPath, DiffError> {
    let bytes = expected_prefix
        .and_then(|prefix| bytes.strip_prefix(prefix))
        .unwrap_or(bytes);
    let path = std::str::from_utf8(bytes).map_err(DiffError::UnsupportedPathEncoding)?;
    RepoPath::new(path).map_err(DiffError::InvalidPath)
}

fn mode_string(mode: FileMode) -> String {
    match mode {
        FileMode::Regular => "100644",
        FileMode::Executable => "100755",
        FileMode::Symlink => "120000",
        FileMode::Gitlink => "160000",
    }
    .to_owned()
}

/// Parses `git status --porcelain=v1 -z` output.
///
/// # Errors
/// Returns an error for malformed records, unsupported path encoding, or
/// invalid repository-relative paths.
pub fn parse_porcelain_v1_z(bytes: &[u8]) -> Result<Vec<GitStatusEntry>, DiffError> {
    let mut fields = bytes.split(|byte| *byte == 0);
    let mut entries = Vec::new();
    while let Some(field) = fields.next() {
        if field.is_empty() {
            continue;
        }
        if field.len() < 3 || field[2] != b' ' {
            return Err(DiffError::InvalidPorcelainEntry);
        }
        let index_column = char::from(field[0]);
        let worktree_column = char::from(field[1]);
        let path = decode_path(&field[3..], None)?;
        let old_path = if matches!(index_column, 'R' | 'C') || matches!(worktree_column, 'R' | 'C')
        {
            let previous = fields.next().filter(|field| !field.is_empty());
            Some(decode_path(
                previous.ok_or(DiffError::InvalidPorcelainEntry)?,
                None,
            )?)
        } else {
            None
        };
        let index = status_column(index_column);
        let worktree = status_column(worktree_column);
        let staged = if index.is_none() || index_column == '?' {
            StageState::Unstaged
        } else if worktree.is_some() {
            StageState::PartiallyStaged
        } else {
            StageState::Staged
        };
        let relevant = if worktree_column == ' ' {
            index_column
        } else {
            worktree_column
        };
        entries.push(GitStatusEntry {
            path,
            old_path,
            status: file_status(relevant),
            staged,
            index,
            worktree,
        });
    }
    Ok(entries)
}

fn status_column(value: char) -> Option<char> {
    (!matches!(value, ' ' | '!')).then_some(value)
}

const fn file_status(value: char) -> FileStatus {
    match value {
        'A' => FileStatus::Added,
        'D' => FileStatus::Deleted,
        'R' => FileStatus::Renamed,
        'C' => FileStatus::Copied,
        '?' => FileStatus::Untracked,
        _ => FileStatus::Modified,
    }
}

impl DiffDocument {
    /// Builds a document from Git patch and porcelain status output.
    ///
    /// # Errors
    /// Returns an error when either Git output cannot be parsed or normalized.
    pub fn from_git_outputs(
        repo_root: impl Into<String>,
        diff: &[u8],
        porcelain: &[u8],
        scope: DiffScope,
    ) -> Result<Self, DiffError> {
        Self::from_git_outputs_with_untracked(repo_root, diff, porcelain, scope, &[])
    }

    /// Builds a document from Git outputs plus separately loaded untracked files.
    ///
    /// # Errors
    /// Returns an error when Git output or untracked file data cannot be parsed
    /// or normalized.
    pub fn from_git_outputs_with_untracked(
        repo_root: impl Into<String>,
        diff: &[u8],
        porcelain: &[u8],
        scope: DiffScope,
        untracked: &[UntrackedFile],
    ) -> Result<Self, DiffError> {
        let statuses = parse_porcelain_v1_z(porcelain)?;
        let mut files = parse_git_diff(diff)?;
        let mut statuses_by_path = HashMap::with_capacity(statuses.len().saturating_mul(2));
        for status in &statuses {
            statuses_by_path
                .entry(status.path.clone())
                .or_insert(status);
            if let Some(old_path) = &status.old_path {
                statuses_by_path.entry(old_path.clone()).or_insert(status);
            }
        }
        let fallback_stage = match scope {
            DiffScope::Staged => StageState::Staged,
            DiffScope::Unstaged | DiffScope::Both => StageState::Unstaged,
        };
        for file in &mut files {
            let status = statuses_by_path.get(&file.path).copied().or_else(|| {
                file.old_path
                    .as_ref()
                    .and_then(|path| statuses_by_path.get(path).copied())
            });
            match status {
                Some(status) => {
                    file.staged = status.staged;
                    if file.status == FileStatus::Modified {
                        file.status = status.status;
                    }
                }
                None => file.staged = fallback_stage,
            }
        }

        for untracked_file in untracked {
            if files.iter().any(|file| file.path == untracked_file.path) {
                continue;
            }
            files.push(untracked_diff(untracked_file)?);
        }

        Ok(Self {
            repo_root: repo_root.into(),
            files,
        })
    }
}

fn untracked_diff(file: &UntrackedFile) -> Result<FileDiff, DiffError> {
    let mut diff = match (file.omitted_bytes, std::str::from_utf8(&file.contents)) {
        (None, Ok(text)) if !text.contains('\0') => {
            FileDiff::from_texts(file.path.clone(), "", text)?
        }
        _ => FileDiff {
            old_path: None,
            path: file.path.clone(),
            status: FileStatus::Untracked,
            staged: StageState::Unstaged,
            hunks: Vec::new(),
            binary: true,
            mode: None,
            no_newline_at_end: false,
            omitted_bytes: file.omitted_bytes,
        },
    };
    diff.status = FileStatus::Untracked;
    diff.staged = StageState::Unstaged;
    Ok(diff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_rename_modes_and_no_newline() {
        let patch = b"diff --git a/old.rs b/new.rs\nsimilarity index 80%\nrename from old.rs\nrename to new.rs\nold mode 100644\nnew mode 100755\n--- a/old.rs\n+++ b/new.rs\n@@ -1 +1 @@ function\n-old\n+new\n\\ No newline at end of file\n";
        let files = parse_git_diff(patch).unwrap();
        assert_eq!(files[0].status, FileStatus::Renamed);
        assert_eq!(files[0].path.as_str(), "new.rs");
        assert_eq!(
            files[0].mode.as_ref().unwrap().new.as_deref(),
            Some("100755")
        );
        assert!(files[0].hunks[0].lines[1].no_newline);
        assert!(files[0].no_newline_at_end);
        assert_eq!(
            files[0].hunks[0].function_context.as_deref(),
            Some("function")
        );
        assert_eq!(files[0].hunks[0].header, "@@ -1 +1 @@ function");
    }

    #[test]
    fn oversized_tracked_patches_are_omitted() {
        let mut patch = String::from(
            "diff --git a/generated.c b/generated.c\n--- a/generated.c\n+++ b/generated.c\n@@ -1,20001 +0,0 @@\n",
        );
        for _ in 0..=MAX_TRACKED_PATCH_LINES {
            patch.push_str("-generated line\n");
        }

        let files = parse_git_diff(patch.as_bytes()).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].hunks.is_empty());
        assert!(files[0].omitted_bytes.is_some());
        assert!(!files[0].binary);
    }

    #[test]
    fn numbers_lines_on_the_sides_they_belong_to() {
        let patch = b"diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,2 @@\n keep\n-old\n+new\n";
        let lines = &parse_git_diff(patch).unwrap()[0].hunks[0].lines;
        assert_eq!(
            (lines[0].old_line_no, lines[0].new_line_no),
            (Some(1), Some(1))
        );
        assert_eq!(
            (lines[1].old_line_no, lines[1].new_line_no),
            (Some(2), None)
        );
        assert_eq!(
            (lines[2].old_line_no, lines[2].new_line_no),
            (None, Some(2))
        );
    }

    #[test]
    fn parses_nul_status_with_spaces_and_rename() {
        let status = b" M src/a file.rs\0R  new.rs\0old.rs\0?? unicode-\xc3\xa9.rs\0";
        let entries = parse_porcelain_v1_z(status).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].staged, StageState::Unstaged);
        assert_eq!(entries[1].staged, StageState::Staged);
        assert_eq!(entries[1].old_path.as_ref().unwrap().as_str(), "old.rs");
        assert_eq!(entries[2].status, FileStatus::Untracked);
        assert_eq!(entries[2].staged, StageState::Unstaged);
    }

    #[test]
    fn parses_copy_binary_mode_only_and_quoted_paths() {
        let patch = b"diff --git a/original.rs b/copied.rs\nsimilarity index 100%\ncopy from original.rs\ncopy to copied.rs\ndiff --git \"a/tab\\tname.bin\" \"b/tab\\tname.bin\"\nindex 1234567..abcdef0 100644\nBinary files \"a/tab\\tname.bin\" and \"b/tab\\tname.bin\" differ\ndiff --git a/script.sh b/script.sh\nold mode 100644\nnew mode 100755\n";
        let files = parse_git_diff(patch).unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].status, FileStatus::Copied);
        assert_eq!(files[0].old_path.as_ref().unwrap().as_str(), "original.rs");
        assert!(files[1].binary);
        assert_eq!(files[1].path.as_str(), "tab\tname.bin");
        assert_eq!(
            files[2].mode.as_ref().unwrap().old.as_deref(),
            Some("100644")
        );
    }

    #[test]
    fn merges_status_and_untracked_text_and_binary() {
        let patch = b"diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
        let document = DiffDocument::from_git_outputs_with_untracked(
            "/repo",
            patch,
            b"MM a.rs\0?? note.txt\0?? data.bin\0",
            DiffScope::Both,
            &[
                UntrackedFile {
                    path: RepoPath::new("note.txt").unwrap(),
                    contents: b"hello\n".to_vec(),
                    omitted_bytes: None,
                },
                UntrackedFile {
                    path: RepoPath::new("data.bin").unwrap(),
                    contents: b"a\0b".to_vec(),
                    omitted_bytes: None,
                },
            ],
        )
        .unwrap();
        assert_eq!(document.files[0].staged, StageState::PartiallyStaged);
        assert_eq!(document.files[1].status, FileStatus::Untracked);
        assert!(!document.files[1].binary);
        assert!(document.files[2].binary);
    }

    #[test]
    fn represents_omitted_untracked_content_without_diffing_it() {
        let document = DiffDocument::from_git_outputs_with_untracked(
            "/repo",
            b"",
            b"?? large.bin\0",
            DiffScope::Unstaged,
            &[UntrackedFile {
                path: RepoPath::new("large.bin").unwrap(),
                contents: Vec::new(),
                omitted_bytes: Some(10_000_000),
            }],
        )
        .unwrap();
        assert!(document.files[0].binary);
        assert_eq!(document.files[0].omitted_bytes, Some(10_000_000));
        assert!(document.files[0].hunks.is_empty());
    }

    #[test]
    fn rejects_non_utf8_path() {
        let patch = b"diff --git a/ok b/\xff\n--- a/ok\n+++ b/\xff\n@@ -1 +1 @@\n-a\n+b\n";
        assert!(matches!(
            parse_git_diff(patch),
            Err(DiffError::UnsupportedPathEncoding(_))
        ));
    }

    #[test]
    fn rejects_malformed_porcelain_records() {
        assert!(matches!(
            parse_porcelain_v1_z(b"XY\0"),
            Err(DiffError::InvalidPorcelainEntry)
        ));
        assert!(matches!(
            parse_porcelain_v1_z(b"R  renamed.rs\0"),
            Err(DiffError::InvalidPorcelainEntry)
        ));
    }
}
