//! Normalization adapters for Git patches and porcelain status output.

use crate::{
    DiffDocument, DiffError, DiffScope, FileDiff, FileStatus, Hunk, ModeChange, PatchLine,
    PatchLineKind, RepoPath, StageState,
};
use diffy::{
    Line,
    patch_set::{FileMode, FileOperation, FilePatch, ParseOptions, PatchKind, PatchSet},
};

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
    /// File bytes from the worktree.
    pub contents: Vec<u8>,
}

/// Parses and normalizes a multi-file Git diff.
///
/// Paths are decoded by `diffy`, stripped of Git's synthetic `a/` and `b/`
/// prefixes, and then validated as UTF-8 repository-relative paths.
pub fn parse_git_diff(bytes: &[u8]) -> Result<Vec<FileDiff>, DiffError> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Vec::new());
    }

    PatchSet::parse_bytes(bytes, ParseOptions::gitdiff())
        .map(|patch| {
            let patch = patch.map_err(|source| DiffError::Parse { source })?;
            normalize_patch(&patch)
        })
        .collect()
}

fn normalize_patch(patch: &FilePatch<'_, [u8]>) -> Result<FileDiff, DiffError> {
    let (old_path, path, status) = match patch.operation() {
        FileOperation::Create(path) => (None, decode_path(path, Some(b"b/"))?, FileStatus::Added),
        FileOperation::Delete(path) => {
            let path = decode_path(path, Some(b"a/"))?;
            (Some(path.clone()), path, FileStatus::Deleted)
        }
        FileOperation::Modify { original, modified } => {
            let original = decode_path(original, Some(b"a/"))?;
            let modified = decode_path(modified, Some(b"b/"))?;
            let status = if original == modified {
                FileStatus::Modified
            } else {
                FileStatus::Renamed
            };
            (Some(original), modified, status)
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
    };

    let binary = patch.patch().is_binary();
    let mut no_newline_at_end = false;
    let hunks = match patch.patch() {
        PatchKind::Text(text) => text
            .hunks()
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
                        let (kind, bytes, old_line_no, new_line_no) = match line {
                            Line::Context(bytes) => {
                                let result = (
                                    PatchLineKind::Context,
                                    *bytes,
                                    Some(old_line),
                                    Some(new_line),
                                );
                                old_line += 1;
                                new_line += 1;
                                result
                            }
                            Line::Delete(bytes) => {
                                let result = (PatchLineKind::Removed, *bytes, Some(old_line), None);
                                old_line += 1;
                                result
                            }
                            Line::Insert(bytes) => {
                                let result = (PatchLineKind::Added, *bytes, None, Some(new_line));
                                new_line += 1;
                                result
                            }
                        };
                        let no_newline = !bytes.ends_with(b"\n");
                        no_newline_at_end |= no_newline;
                        let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
                        let text = std::str::from_utf8(bytes)?.to_owned();
                        Ok(PatchLine {
                            kind,
                            text,
                            old_line_no,
                            new_line_no,
                            no_newline,
                        })
                    })
                    .collect::<Result<Vec<_>, DiffError>>()?;
                let function_context = hunk
                    .function_context()
                    .map(|bytes| std::str::from_utf8(bytes).map(str::to_owned))
                    .transpose()?;
                let header = format!(
                    "@@ -{} +{} @@{}",
                    old,
                    new,
                    function_context
                        .as_deref()
                        .map_or(String::new(), |context| format!(" {context}"))
                );
                Ok(Hunk {
                    header,
                    function_context,
                    old_start: old.start(),
                    old_count: old.len(),
                    new_start: new.start(),
                    new_count: new.len(),
                    lines,
                })
            })
            .collect::<Result<Vec<_>, DiffError>>()?,
        PatchKind::Binary(_) => Vec::new(),
    };

    let old_mode = patch.old_mode().map(mode_string);
    let new_mode = patch.new_mode().map(mode_string);
    let mode = (old_mode != new_mode && (old_mode.is_some() || new_mode.is_some())).then_some(
        ModeChange {
            old: old_mode,
            new: new_mode,
        },
    );

    Ok(FileDiff {
        old_path,
        path,
        status,
        staged: StageState::Unstaged,
        hunks,
        binary,
        mode,
        no_newline_at_end,
    })
}

fn decode_path(bytes: &[u8], expected_prefix: Option<&[u8]>) -> Result<RepoPath, DiffError> {
    let bytes = expected_prefix
        .and_then(|prefix| bytes.strip_prefix(prefix))
        .unwrap_or(bytes);
    let path = std::str::from_utf8(bytes).map_err(DiffError::UnsupportedPathEncoding)?;
    RepoPath::new(path).map_err(DiffError::InvalidPath)
}

fn mode_string(mode: &FileMode) -> String {
    match mode {
        FileMode::Regular => "100644",
        FileMode::Executable => "100755",
        FileMode::Symlink => "120000",
        FileMode::Gitlink => "160000",
    }
    .to_owned()
}

/// Parses `git status --porcelain=v1 -z` output without treating paths as
/// whitespace-delimited text.
pub fn parse_porcelain_v1_z(bytes: &[u8]) -> Result<Vec<GitStatusEntry>, DiffError> {
    let mut fields = bytes.split(|byte| *byte == 0).peekable();
    let mut entries = Vec::new();
    while let Some(field) = fields.next() {
        if field.is_empty() {
            continue;
        }
        if field.len() < 3 || field[2] != b' ' {
            return Err(DiffError::InvalidPorcelainEntry);
        }
        let x = field[0] as char;
        let y = field[1] as char;
        let path = decode_path(&field[3..], None)?;
        let renamed_or_copied = matches!(x, 'R' | 'C') || matches!(y, 'R' | 'C');
        let old_path = if renamed_or_copied {
            let old = fields.next().ok_or(DiffError::InvalidPorcelainEntry)?;
            Some(decode_path(old, None)?)
        } else {
            None
        };
        let index = status_column(x);
        let worktree = status_column(y);
        let staged = match (index, worktree) {
            (Some(_), Some(_)) if x != '?' => StageState::PartiallyStaged,
            (Some(_), _) if x != '?' => StageState::Staged,
            _ => StageState::Unstaged,
        };
        let relevant = if y != ' ' { y } else { x };
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

fn file_status(value: char) -> FileStatus {
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
    /// Builds a canonical snapshot from Git diff and porcelain status output.
    pub fn from_git_outputs(
        repo_root: impl Into<String>,
        diff: &[u8],
        porcelain: &[u8],
        scope: DiffScope,
    ) -> Result<Self, DiffError> {
        Self::from_git_outputs_with_untracked(repo_root, diff, porcelain, scope, &[])
    }

    /// Builds a snapshot and appends host-provided untracked file contents.
    pub fn from_git_outputs_with_untracked(
        repo_root: impl Into<String>,
        diff: &[u8],
        porcelain: &[u8],
        scope: DiffScope,
        untracked: &[UntrackedFile],
    ) -> Result<Self, DiffError> {
        let statuses = parse_porcelain_v1_z(porcelain)?;
        let mut files = parse_git_diff(diff)?;
        for file in &mut files {
            if let Some(status) = statuses.iter().find(|status| {
                status.path == file.path
                    || status.old_path.as_ref() == Some(&file.path)
                    || file.old_path.as_ref() == Some(&status.path)
            }) {
                file.staged = status.staged;
                if file.status == FileStatus::Modified {
                    file.status = status.status;
                }
            } else {
                file.staged = match scope {
                    DiffScope::Staged => StageState::Staged,
                    DiffScope::Unstaged | DiffScope::Both => StageState::Unstaged,
                };
            }
        }

        for untracked_file in untracked {
            if files.iter().any(|file| file.path == untracked_file.path) {
                continue;
            }
            let mut file = match std::str::from_utf8(&untracked_file.contents) {
                Ok(text) if !text.contains('\0') => {
                    FileDiff::from_texts(untracked_file.path.clone(), "", text)?
                }
                _ => FileDiff {
                    old_path: None,
                    path: untracked_file.path.clone(),
                    status: FileStatus::Untracked,
                    staged: StageState::Unstaged,
                    hunks: Vec::new(),
                    binary: true,
                    mode: None,
                    no_newline_at_end: false,
                },
            };
            file.status = FileStatus::Untracked;
            file.staged = StageState::Unstaged;
            files.push(file);
        }

        Ok(Self {
            repo_root: repo_root.into(),
            files,
        })
    }
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
    }

    #[test]
    fn parses_nul_status_with_spaces_and_rename() {
        let status = b" M src/a file.rs\0R  new.rs\0old.rs\0?? unicode-\xc3\xa9.rs\0";
        let entries = parse_porcelain_v1_z(status).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].staged, StageState::Unstaged);
        assert_eq!(entries[1].old_path.as_ref().unwrap().as_str(), "old.rs");
        assert_eq!(entries[2].status, FileStatus::Untracked);
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
                },
                UntrackedFile {
                    path: RepoPath::new("data.bin").unwrap(),
                    contents: b"a\0b".to_vec(),
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
    fn rejects_non_utf8_path() {
        let patch = b"diff --git a/ok b/\xff\n--- a/ok\n+++ b/\xff\n@@ -1 +1 @@\n-a\n+b\n";
        assert!(matches!(
            parse_git_diff(patch),
            Err(DiffError::UnsupportedPathEncoding(_))
        ));
    }
}
