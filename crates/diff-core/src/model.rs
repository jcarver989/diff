//! Owned, serializable diff domain types.

use crate::{DiffError, ParseDiffScopeError, RepoPathError};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use similar::{ChangeTag, TextDiff};
use std::{fmt, path::Path, str::FromStr, sync::Arc};

/// A validated UTF-8 path relative to a repository root.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RepoPath(Arc<str>);

impl RepoPath {
    /// Validates and stores a repository-relative UTF-8 path.
    ///
    /// # Errors
    /// Returns an error for empty, absolute, traversing, or NUL-containing paths.
    pub fn new(path: impl AsRef<str>) -> Result<Self, RepoPathError> {
        let raw = path.as_ref();
        if raw.is_empty() {
            return Err(RepoPathError::Empty);
        }
        if raw.bytes().any(|byte| byte == 0) {
            return Err(RepoPathError::Nul);
        }
        if raw.starts_with('/') || raw.starts_with("\\\\") || raw.as_bytes().get(1) == Some(&b':') {
            return Err(RepoPathError::Absolute);
        }
        for component in raw.split('/') {
            match component {
                "" => return Err(RepoPathError::Empty),
                "." | ".." => return Err(RepoPathError::Traversal),
                _ => {}
            }
        }
        Ok(Self(Arc::from(raw)))
    }

    /// Returns the validated Git path.
    #[must_use]
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
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for RepoPath {
    type Error = RepoPathError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl Serialize for RepoPath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RepoPath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// The side of a patch to which a line or comment belongs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum DiffSide {
    Old,
    New,
}

impl DiffSide {
    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::Old => Self::New,
            Self::New => Self::Old,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Old => "old",
            Self::New => "new",
        }
    }
}

/// The snapshot included in a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DiffScope {
    Unstaged,
    Staged,
    #[default]
    Both,
}

impl DiffScope {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unstaged => "unstaged",
            Self::Staged => "staged",
            Self::Both => "both",
        }
    }

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Unstaged => Self::Staged,
            Self::Staged => Self::Both,
            Self::Both => Self::Unstaged,
        }
    }
}

impl fmt::Display for DiffScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DiffScope {
    type Err = ParseDiffScopeError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "unstaged" => Ok(Self::Unstaged),
            "staged" => Ok(Self::Staged),
            "both" => Ok(Self::Both),
            other => Err(ParseDiffScopeError(other.to_owned())),
        }
    }
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

impl FileStatus {
    #[must_use]
    pub const fn code(self) -> char {
        match self {
            Self::Modified => 'M',
            Self::Added => 'A',
            Self::Deleted => 'D',
            Self::Renamed => 'R',
            Self::Copied => 'C',
            Self::Untracked => '?',
        }
    }
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

impl PatchLineKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HunkHeader => "HunkHeader",
            Self::Context => "Context",
            Self::Added => "Added",
            Self::Removed => "Removed",
            Self::Meta => "Meta",
        }
    }

    #[must_use]
    pub const fn sides(self) -> &'static [DiffSide] {
        match self {
            Self::Removed => &[DiffSide::Old],
            Self::Added => &[DiffSide::New],
            _ => &[DiffSide::Old, DiffSide::New],
        }
    }

    /// Semantic tint used when presenting a line of this kind.
    #[must_use]
    pub const fn tone(self) -> diff_theme::DiffTone {
        match self {
            Self::Added => diff_theme::DiffTone::Added,
            Self::Removed => diff_theme::DiffTone::Removed,
            Self::Meta | Self::HunkHeader => diff_theme::DiffTone::Meta,
            Self::Context => diff_theme::DiffTone::Context,
        }
    }
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

impl DiffDocument {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            repo_root: String::new(),
            files: Vec::new(),
        }
    }

    #[must_use]
    pub fn file_index(&self, path: &RepoPath) -> Option<usize> {
        self.files.iter().position(|file| &file.path == path)
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub omitted_bytes: Option<u64>,
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
    pub text: Arc<str>,
    pub old_line_no: Option<usize>,
    pub new_line_no: Option<usize>,
    pub no_newline: bool,
}

impl PatchLine {
    /// Creates an added line.
    #[must_use]
    pub fn added(text: impl Into<Arc<str>>, new_line_no: usize) -> Self {
        Self {
            kind: PatchLineKind::Added,
            text: text.into(),
            old_line_no: None,
            new_line_no: Some(new_line_no),
            no_newline: false,
        }
    }

    /// Creates a removed line.
    #[must_use]
    pub fn removed(text: impl Into<Arc<str>>, old_line_no: usize) -> Self {
        Self {
            kind: PatchLineKind::Removed,
            text: text.into(),
            old_line_no: Some(old_line_no),
            new_line_no: None,
            no_newline: false,
        }
    }

    /// Creates a context line.
    #[must_use]
    pub fn context(text: impl Into<Arc<str>>, old_line_no: usize, new_line_no: usize) -> Self {
        Self {
            kind: PatchLineKind::Context,
            text: text.into(),
            old_line_no: Some(old_line_no),
            new_line_no: Some(new_line_no),
            no_newline: false,
        }
    }

    /// Marks this line as the final line without a trailing newline.
    #[must_use]
    pub fn without_newline(mut self) -> Self {
        self.no_newline = true;
        self
    }

    /// Returns the number for a side.
    #[must_use]
    pub const fn line_number(&self, side: DiffSide) -> Option<usize> {
        match side {
            DiffSide::Old => self.old_line_no,
            DiffSide::New => self.new_line_no,
        }
    }
}

impl Hunk {
    /// Number of additions.
    #[must_use]
    pub fn additions(&self) -> usize {
        self.count(PatchLineKind::Added)
    }

    /// Number of deletions.
    #[must_use]
    pub fn deletions(&self) -> usize {
        self.count(PatchLineKind::Removed)
    }

    fn count(&self, kind: PatchLineKind) -> usize {
        self.lines.iter().filter(|line| line.kind == kind).count()
    }
}

impl FileDiff {
    /// Builds a one-file diff from complete old and new text snapshots.
    ///
    /// # Errors
    /// Returns an error when `path` is not a valid repository-relative path.
    pub fn from_texts<T>(path: T, old: &str, new: &str) -> Result<Self, DiffError>
    where
        T: TryInto<RepoPath>,
        T::Error: Into<RepoPathError>,
    {
        let path = path
            .try_into()
            .map_err(|error| DiffError::InvalidPath(error.into()))?;
        let status = match (old.is_empty(), new.is_empty()) {
            (true, false) => FileStatus::Added,
            (false, true) => FileStatus::Deleted,
            _ => FileStatus::Modified,
        };

        let mut lines = Vec::new();
        let (mut old_line, mut new_line) = (0, 0);
        for change in TextDiff::from_lines(old, new).iter_all_changes() {
            let text = strip_line_ending(change.value());
            lines.push(match change.tag() {
                ChangeTag::Equal => {
                    old_line += 1;
                    new_line += 1;
                    PatchLine::context(text, old_line, new_line)
                }
                ChangeTag::Delete => {
                    old_line += 1;
                    PatchLine::removed(text, old_line)
                }
                ChangeTag::Insert => {
                    new_line += 1;
                    PatchLine::added(text, new_line)
                }
            });
        }

        let changed = lines
            .iter()
            .any(|line| matches!(line.kind, PatchLineKind::Added | PatchLineKind::Removed));
        let mut hunks = if changed {
            vec![whole_file_hunk(lines)]
        } else {
            Vec::new()
        };

        let old_incomplete = !old.is_empty() && !old.ends_with('\n');
        let new_incomplete = !new.is_empty() && !new.ends_with('\n');
        if let Some(hunk) = hunks.last_mut() {
            if new_incomplete {
                mark_last_incomplete(hunk, PatchLineKind::Added);
            }
            if old_incomplete {
                mark_last_incomplete(hunk, PatchLineKind::Removed);
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
            no_newline_at_end: old_incomplete || new_incomplete,
            omitted_bytes: None,
        })
    }

    /// Number of added lines.
    #[must_use]
    pub fn additions(&self) -> usize {
        self.hunks.iter().map(Hunk::additions).sum()
    }

    /// Number of removed lines.
    #[must_use]
    pub fn deletions(&self) -> usize {
        self.hunks.iter().map(Hunk::deletions).sum()
    }

    /// Lowercase file extension, without a dot.
    #[must_use]
    pub fn language(&self) -> &str {
        Path::new(self.path.as_str())
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default()
    }

    #[must_use]
    pub fn line(&self, hunk: usize, line: usize) -> Option<&PatchLine> {
        self.hunks.get(hunk)?.lines.get(line)
    }
}

fn strip_line_ending(value: &str) -> &str {
    value.strip_suffix('\n').map_or(value, |trimmed| {
        trimmed.strip_suffix('\r').unwrap_or(trimmed)
    })
}

fn whole_file_hunk(lines: Vec<PatchLine>) -> Hunk {
    let old_start = lines.iter().find_map(|line| line.old_line_no).unwrap_or(0);
    let new_start = lines.iter().find_map(|line| line.new_line_no).unwrap_or(0);
    let old_count = lines
        .iter()
        .filter(|line| line.old_line_no.is_some())
        .count();
    let new_count = lines
        .iter()
        .filter(|line| line.new_line_no.is_some())
        .count();
    Hunk {
        header: format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@"),
        function_context: None,
        old_start,
        old_count,
        new_start,
        new_count,
        lines,
    }
}

fn mark_last_incomplete(hunk: &mut Hunk, kind: PatchLineKind) {
    if let Some(line) = hunk.lines.iter_mut().rev().find(|line| line.kind == kind) {
        line.no_newline = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_relative_utf8_paths() {
        assert!(RepoPath::new("src/é file.rs").is_ok());
        assert_eq!(RepoPath::new("../secret"), Err(RepoPathError::Traversal));
        assert_eq!(RepoPath::new("/absolute"), Err(RepoPathError::Absolute));
        assert_eq!(RepoPath::new("a//b"), Err(RepoPathError::Empty));
        assert_eq!(RepoPath::new("a\0b"), Err(RepoPathError::Nul));
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
    fn strips_carriage_returns_like_str_lines() {
        let diff = FileDiff::from_texts("main.rs", "a\r\n", "b\r\n").unwrap();
        assert_eq!(diff.hunks[0].lines[0].text.as_ref(), "a");
        assert_eq!(diff.hunks[0].lines[1].text.as_ref(), "b");
    }

    #[test]
    fn parses_and_renders_scopes() {
        assert_eq!("staged".parse::<DiffScope>().unwrap(), DiffScope::Staged);
        assert_eq!(DiffScope::Both.to_string(), "both");
        assert!("nope".parse::<DiffScope>().is_err());
        assert_eq!(DiffScope::Both.next(), DiffScope::Unstaged);
    }

    #[test]
    fn serializes_owned_model() {
        let document = DiffDocument::empty();
        let json = serde_json::to_string(&document).unwrap();
        assert_eq!(
            serde_json::from_str::<DiffDocument>(&json).unwrap(),
            document
        );
    }
}
