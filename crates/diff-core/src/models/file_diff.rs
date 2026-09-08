use super::{DiffSide, Hunk, PatchLine, RepoPath, patch_derivation::derive_patch};
use crate::{
    DiffError, Fingerprint, RepoPathError, SourceDocument, SourceResult, SourceUnavailable,
};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, ffi::OsStr, path::Path, sync::Arc};

const FILE_CONTENT_DOMAIN: &[u8] = b"diff-file-content-v1";

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

/// A file mode change, represented as Git's six-digit mode string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeChange {
    pub old: Option<String>,
    pub new: Option<String>,
}

/// One changed file.
///
/// `old_source` and `new_source` hold the complete versions of each side.
/// When both are available the hunks are derived from them, so equal sides
/// imply equal hunks; otherwise the hunks are the patch as supplied.
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
    #[serde(default = "not_captured", skip_serializing_if = "is_not_captured")]
    pub old_source: SourceResult,
    #[serde(default = "not_captured", skip_serializing_if = "is_not_captured")]
    pub new_source: SourceResult,
}

fn not_captured() -> SourceResult {
    Err(SourceUnavailable::NotCaptured)
}

fn is_not_captured(source: &SourceResult) -> bool {
    matches!(source, Err(SourceUnavailable::NotCaptured))
}

impl FileDiff {
    /// Returns the repository path used by one side of this file version.
    #[must_use]
    pub fn path_for_side(&self, side: DiffSide) -> &RepoPath {
        match side {
            DiffSide::Old => self.old_path.as_ref().unwrap_or(&self.path),
            DiffSide::New => &self.path,
        }
    }

    /// Builds a one-file diff from complete old and new text snapshots.
    ///
    /// # Errors
    ///
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
        let (hunks, no_newline_at_end) = derive_patch(old, new);
        let side = |text: &str, absent: bool| {
            if absent {
                Err(SourceUnavailable::Absent)
            } else {
                SourceDocument::new(text).map(Arc::new)
            }
        };
        Ok(Self {
            old_path: (status != FileStatus::Added).then(|| path.clone()),
            path,
            status,
            staged: StageState::Unstaged,
            hunks,
            binary: false,
            mode: None,
            no_newline_at_end,
            omitted_bytes: None,
            old_source: side(old, status == FileStatus::Added),
            new_source: side(new, status == FileStatus::Deleted),
        })
    }

    /// Attaches captured source versions, re-deriving the hunks from them when
    /// both sides are known. Git metadata such as status, staging, mode, and
    /// paths is kept as supplied; a binary file keeps its patch untouched.
    #[must_use]
    pub fn with_sources(mut self, old: SourceResult, new: SourceResult) -> Self {
        self.old_source = old;
        self.new_source = new;
        if self.binary {
            return self;
        }
        if let (Some(old), Some(new)) =
            (self.side_text(DiffSide::Old), self.side_text(DiffSide::New))
        {
            let (hunks, no_newline_at_end) = derive_patch(old, new);
            self.hunks = hunks;
            self.no_newline_at_end = no_newline_at_end;
        }
        self
    }

    /// The source result recorded for a side of a file whose complete versions
    /// were not captured.
    ///
    /// # Errors
    /// Always an unavailable reason: [`SourceUnavailable::Absent`] where the
    /// status says the side cannot exist, [`SourceUnavailable::NotCaptured`]
    /// otherwise.
    pub const fn uncaptured_source(status: FileStatus, side: DiffSide) -> SourceResult {
        if Self::side_is_absent(status, side) {
            Err(SourceUnavailable::Absent)
        } else {
            Err(SourceUnavailable::NotCaptured)
        }
    }

    const fn side_is_absent(status: FileStatus, side: DiffSide) -> bool {
        matches!(
            (status, side),
            (FileStatus::Added | FileStatus::Untracked, DiffSide::Old)
                | (FileStatus::Deleted, DiffSide::New)
        )
    }

    /// Returns the complete source result for one side.
    pub const fn source(&self, side: DiffSide) -> &SourceResult {
        match side {
            DiffSide::Old => &self.old_source,
            DiffSide::New => &self.new_source,
        }
    }

    /// Returns the complete source document for one side when it is available.
    #[must_use]
    pub fn source_document(&self, side: DiffSide) -> Option<&Arc<SourceDocument>> {
        self.source(side).as_ref().ok()
    }

    /// Returns why one side has no complete source document.
    #[must_use]
    pub fn source_unavailable(&self, side: DiffSide) -> Option<&SourceUnavailable> {
        self.source(side).as_ref().err()
    }

    /// Complete text for a side: the captured source, or empty where the
    /// status says the side does not exist.
    fn side_text(&self, side: DiffSide) -> Option<&str> {
        match self.source(side) {
            Ok(source) => Some(source.text()),
            Err(SourceUnavailable::Absent) if Self::side_is_absent(self.status, side) => Some(""),
            Err(_) => None,
        }
    }

    /// Content identity used to decide whether presentation state derived from
    /// this file, such as revealed gaps, still applies after a replacement.
    ///
    /// The identity covers both sides. When either side is unavailable the
    /// hunks are the content and are covered too; when both are available the
    /// hunks are derived from them and add nothing.
    #[must_use]
    pub fn content_id(&self) -> Fingerprint {
        let mut fields: Vec<Cow<'_, [u8]>> = vec![
            Cow::Borrowed(FILE_CONTENT_DOMAIN),
            Cow::Owned(vec![
                u8::try_from(self.status.code()).unwrap_or(b'?'),
                u8::from(self.binary),
            ]),
        ];
        let mut complete = true;
        for side in [DiffSide::Old, DiffSide::New] {
            match self.source(side) {
                Ok(source) => fields.push(Cow::Owned(source.content_id().as_bytes().to_vec())),
                Err(reason) => {
                    complete = false;
                    fields.push(Cow::Owned(reason.to_string().into_bytes()));
                }
            }
        }
        if !complete {
            for hunk in &self.hunks {
                fields.push(Cow::Borrowed(hunk.header.as_bytes()));
                for line in &hunk.lines {
                    fields.push(Cow::Borrowed(line.kind.as_str().as_bytes()));
                    fields.push(Cow::Borrowed(line.text.as_bytes()));
                    fields.push(Cow::Owned(
                        [line.old_line_no, line.new_line_no]
                            .iter()
                            .flat_map(|number| number.unwrap_or(0).to_le_bytes())
                            .chain([u8::from(line.no_newline)])
                            .collect(),
                    ));
                }
            }
        }
        Fingerprint::of(fields)
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
            .and_then(OsStr::to_str)
            .unwrap_or_default()
    }

    #[must_use]
    pub fn line(&self, hunk: usize, line: usize) -> Option<&PatchLine> {
        self.hunks.get(hunk)?.lines.get(line)
    }
}
