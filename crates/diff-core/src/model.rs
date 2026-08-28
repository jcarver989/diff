//! Owned, serializable diff domain types.

use crate::{DiffError, RepoPathError};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use similar::{DiffOp, TextDiff};
use std::{fmt, path::Path, sync::Arc};

/// A validated UTF-8 path relative to a repository root.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RepoPath(Arc<str>);
impl RepoPath {
    /// Validates and constructs a UTF-8 Git path without normalizing its bytes.
    pub fn new(path: impl AsRef<str>) -> Result<Self, RepoPathError> {
        let raw = path.as_ref();
        if raw.is_empty() {
            return Err(RepoPathError::Empty);
        }

        if raw.bytes().any(|b| b == 0) {
            return Err(RepoPathError::Nul);
        }
        if raw.starts_with('/') || raw.starts_with("\\\\") || (raw.as_bytes().get(1) == Some(&b':'))
        {
            return Err(RepoPathError::Absolute);
        }
        if raw
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(if raw.split('/').any(|part| part == ".." || part == ".") {
                RepoPathError::Traversal
            } else {
                RepoPathError::Empty
            });
        }
        Ok(Self(Arc::from(raw)))
    }
    /// Returns the validated Git path.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}
impl AsRef<str> for RepoPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl TryFrom<String> for RepoPath {
    type Error = RepoPathError;
    fn try_from(v: String) -> Result<Self, Self::Error> {
        Self::new(v)
    }
}
impl TryFrom<&str> for RepoPath {
    type Error = RepoPathError;
    fn try_from(v: &str) -> Result<Self, Self::Error> {
        Self::new(v)
    }
}
impl Serialize for RepoPath {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for RepoPath {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

/// The side of a patch to which a line or comment belongs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum DiffSide {
    Old,
    New,
}
/// The snapshot included in a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DiffScope {
    Unstaged,
    Staged,
    #[default]
    Both,
}
/// Git's file operation classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
}
/// Index/worktree staging state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageState {
    Unstaged,
    Staged,
    PartiallyStaged,
}
/// A line's semantic patch kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PatchLineKind {
    HunkHeader,
    Context,
    Added,
    Removed,
    Meta,
}
/// A file mode change, represented as Git's six-digit mode string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeChange {
    pub old: Option<String>,
    pub new: Option<String>,
}
/// A complete renderer-neutral diff snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffDocument {
    pub repo_root: String,
    pub files: Vec<FileDiff>,
}
/// One changed file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDiff {
    pub old_path: Option<RepoPath>,
    pub path: RepoPath,
    pub status: FileStatus,
    pub staged: StageState,
    pub hunks: Vec<Hunk>,
    pub binary: bool,
    pub mode: Option<ModeChange>,
    pub no_newline_at_end: bool,
}
/// A unified-diff hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    pub header: String,
    pub function_context: Option<String>,
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<PatchLine>,
}
/// A line in a hunk, including canonical old/new numbering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchLine {
    pub kind: PatchLineKind,
    pub text: String,
    pub old_line_no: Option<usize>,
    pub new_line_no: Option<usize>,
    pub no_newline: bool,
}
impl PatchLine {
    /// Creates an added line.
    pub fn added(text: impl Into<String>, n: usize) -> Self {
        Self {
            kind: PatchLineKind::Added,
            text: text.into(),
            old_line_no: None,
            new_line_no: Some(n),
            no_newline: false,
        }
    }
    /// Creates a removed line.
    pub fn removed(text: impl Into<String>, n: usize) -> Self {
        Self {
            kind: PatchLineKind::Removed,
            text: text.into(),
            old_line_no: Some(n),
            new_line_no: None,
            no_newline: false,
        }
    }
    /// Creates a context line.
    pub fn context(text: impl Into<String>, old: usize, new: usize) -> Self {
        Self {
            kind: PatchLineKind::Context,
            text: text.into(),
            old_line_no: Some(old),
            new_line_no: Some(new),
            no_newline: false,
        }
    }
    /// Marks this line as the final line without a trailing newline.
    pub fn without_newline(mut self) -> Self {
        self.no_newline = true;
        self
    }
    /// Returns the number for a side.
    pub fn line_number(&self, side: DiffSide) -> Option<usize> {
        match side {
            DiffSide::Old => self.old_line_no,
            DiffSide::New => self.new_line_no,
        }
    }
}
impl Hunk {
    /// Number of additions.
    pub fn additions(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| l.kind == PatchLineKind::Added)
            .count()
    }
    /// Number of deletions.
    pub fn deletions(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| l.kind == PatchLineKind::Removed)
            .count()
    }
}
impl FileDiff {
    /// Builds a normalized diff from two UTF-8 file contents.
    pub fn from_texts<P>(path: P, old: &str, new: &str) -> Result<Self, DiffError>
    where
        P: TryInto<RepoPath>,
        P::Error: Into<RepoPathError>,
    {
        let path = path
            .try_into()
            .map_err(|e| DiffError::InvalidPath(e.into()))?;
        let status = if old.is_empty() && !new.is_empty() {
            FileStatus::Added
        } else if !old.is_empty() && new.is_empty() {
            FileStatus::Deleted
        } else {
            FileStatus::Modified
        };
        let olds: Vec<&str> = old.lines().collect();
        let news: Vec<&str> = new.lines().collect();
        let mut lines = Vec::new();
        let (mut o, mut n) = (0, 0);
        for op in TextDiff::from_lines(old, new).ops() {
            match *op {
                DiffOp::Equal { old_index, len, .. } => {
                    for i in 0..len {
                        o += 1;
                        n += 1;
                        lines.push(PatchLine::context(
                            olds.get(old_index + i).copied().unwrap_or_default(),
                            o,
                            n,
                        ));
                    }
                }
                DiffOp::Delete {
                    old_index, old_len, ..
                } => {
                    for i in 0..old_len {
                        o += 1;
                        lines.push(PatchLine::removed(
                            olds.get(old_index + i).copied().unwrap_or_default(),
                            o,
                        ));
                    }
                }
                DiffOp::Insert {
                    new_index, new_len, ..
                } => {
                    for i in 0..new_len {
                        n += 1;
                        lines.push(PatchLine::added(
                            news.get(new_index + i).copied().unwrap_or_default(),
                            n,
                        ));
                    }
                }
                DiffOp::Replace {
                    old_index,
                    old_len,
                    new_index,
                    new_len,
                } => {
                    for i in 0..old_len {
                        o += 1;
                        lines.push(PatchLine::removed(
                            olds.get(old_index + i).copied().unwrap_or_default(),
                            o,
                        ));
                    }
                    for i in 0..new_len {
                        n += 1;
                        lines.push(PatchLine::added(
                            news.get(new_index + i).copied().unwrap_or_default(),
                            n,
                        ));
                    }
                }
            }
        }
        let mut hunks = if lines
            .iter()
            .any(|l| matches!(l.kind, PatchLineKind::Added | PatchLineKind::Removed))
        {
            let os = lines.iter().find_map(|l| l.old_line_no).unwrap_or(0);
            let ns = lines.iter().find_map(|l| l.new_line_no).unwrap_or(0);
            let oc = lines.iter().filter(|l| l.old_line_no.is_some()).count();
            let nc = lines.iter().filter(|l| l.new_line_no.is_some()).count();
            let header = format!("@@ -{os},{oc} +{ns},{nc} @@");
            vec![Hunk {
                header: header.clone(),
                function_context: None,
                old_start: os,
                old_count: oc,
                new_start: ns,
                new_count: nc,
                lines: std::mem::take(&mut lines),
            }]
        } else {
            Vec::new()
        };
        let no_newline_at_end =
            (!old.is_empty() && !old.ends_with('\n')) || (!new.is_empty() && !new.ends_with('\n'));
        if no_newline_at_end {
            if let Some(hunk) = hunks.last_mut() {
                if let Some(line) = hunk
                    .lines
                    .iter_mut()
                    .rev()
                    .find(|line| line.kind == PatchLineKind::Added && !new.ends_with('\n'))
                {
                    line.no_newline = true;
                }
                if let Some(line) = hunk
                    .lines
                    .iter_mut()
                    .rev()
                    .find(|line| line.kind == PatchLineKind::Removed && !old.ends_with('\n'))
                {
                    line.no_newline = true;
                }
            }
        }
        Ok(Self {
            old_path: (status != FileStatus::Added).then(|| path.clone()),
            path,
            status,
            staged: StageState::Unstaged,
            hunks,
            binary: false,
            mode: None,
            no_newline_at_end,
        })
    }
    /// Number of added lines.
    pub fn additions(&self) -> usize {
        self.hunks.iter().map(Hunk::additions).sum()
    }
    /// Number of removed lines.
    pub fn deletions(&self) -> usize {
        self.hunks.iter().map(Hunk::deletions).sum()
    }
    /// Lowercase file extension, without a dot.
    pub fn language(&self) -> &str {
        Path::new(self.path.as_str())
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_relative_utf8_paths() {
        assert!(RepoPath::new("src/é file.rs").is_ok());
        assert!(RepoPath::new("../secret").is_err());
        assert!(RepoPath::new("/absolute").is_err());
        assert!(RepoPath::new("a//b").is_err());
        assert_eq!(RepoPath::new("a\\\\b").unwrap().as_str(), "a\\\\b");
    }

    #[test]
    fn builds_canonical_numbered_diff() {
        let diff = FileDiff::from_texts("main.rs", "keep\nold", "keep\nnew").unwrap();
        assert_eq!(diff.status, FileStatus::Modified);
        assert_eq!((diff.additions(), diff.deletions()), (1, 1));
        assert!(diff.no_newline_at_end);
        assert_eq!(diff.hunks[0].lines[0].kind, PatchLineKind::Context);
        assert_eq!(diff.hunks[0].lines[2].new_line_no, Some(2));
    }

    #[test]
    fn serializes_owned_model() {
        let document = DiffDocument {
            repo_root: "/repo".into(),
            files: vec![],
        };
        let json = serde_json::to_string(&document).unwrap();
        assert_eq!(
            serde_json::from_str::<DiffDocument>(&json).unwrap(),
            document
        );
    }
}
