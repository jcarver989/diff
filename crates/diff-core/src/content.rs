//! Immutable complete-file source versions and host-neutral source transport.

use crate::{DiffSide, FileDiff, Fingerprint, RepoPath, SourceSequenceId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Maximum UTF-8 bytes accepted for one complete source version.
pub const MAX_SOURCE_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// Defensive maximum normalized lines accepted for one complete source version.
pub const MAX_SOURCE_FILE_LINES: usize = 1_000_000;

/// Identifies one side of one changed file in a review snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceKey {
    /// The current path used to identify the file in the review sidebar.
    pub review_path: RepoPath,
    pub side: DiffSide,
}

/// A one-based source coordinate independent of patch provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceLineRef {
    pub side: DiffSide,
    pub line_number: usize,
}

/// A durable source coordinate within the current review snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceLocation {
    pub path: RepoPath,
    pub side: DiffSide,
    pub line_number: usize,
}

/// Immutable normalized lines for one exact file version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileVersionText {
    lines: Arc<[Arc<str>]>,
    content_id: Fingerprint,
    sequence_id: SourceSequenceId,
    byte_len: u64,
    trailing_newline: bool,
}

impl SourceKey {
    #[must_use]
    pub fn new(review_path: RepoPath, side: DiffSide) -> Self {
        Self { review_path, side }
    }
}

impl FileVersionText {
    /// Captures exact byte identity while normalizing rendered line endings like patch parsing.
    ///
    /// # Errors
    /// Returns a typed unavailable reason before allocating normalized lines when the source
    /// exceeds the complete-file byte or line limits.
    pub fn try_from_text(text: &str) -> Result<Self, SourceUnavailable> {
        let byte_len = u64::try_from(text.len()).unwrap_or(u64::MAX);
        if byte_len > MAX_SOURCE_FILE_BYTES {
            return Err(SourceUnavailable::TooLarge { bytes: byte_len });
        }
        let line_count = text.split_terminator('\n').count();
        if line_count > MAX_SOURCE_FILE_LINES {
            return Err(SourceUnavailable::TooManyLines { lines: line_count });
        }
        let trailing_newline = text.ends_with('\n');
        let lines: Arc<[Arc<str>]> = if text.is_empty() {
            Arc::from([])
        } else {
            text.split_terminator('\n')
                .map(|line| line.strip_suffix('\r').unwrap_or(line))
                .map(Arc::<str>::from)
                .collect::<Vec<_>>()
                .into()
        };
        let sequence_id = SourceSequenceId::from_lines(lines.iter().map(AsRef::as_ref));
        Ok(Self {
            lines,
            content_id: Fingerprint::of([b"diff-file-version-v1".as_slice(), text.as_bytes()]),
            sequence_id,
            byte_len,
            trailing_newline,
        })
    }

    /// Returns a one-based normalized source line.
    #[must_use]
    pub fn line(&self, number: usize) -> Option<&str> {
        self.line_arc(number).map(AsRef::as_ref)
    }

    /// Returns the stored allocation for a one-based normalized source line.
    #[must_use]
    pub fn line_arc(&self, number: usize) -> Option<&Arc<str>> {
        number
            .checked_sub(1)
            .and_then(|index| self.lines.get(index))
    }

    #[must_use]
    pub fn lines(&self) -> &[Arc<str>] {
        &self.lines
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    #[must_use]
    pub const fn content_id(&self) -> Fingerprint {
        self.content_id
    }

    #[must_use]
    pub const fn sequence_id(&self) -> SourceSequenceId {
        self.sequence_id
    }

    #[must_use]
    pub const fn byte_len(&self) -> u64 {
        self.byte_len
    }

    #[must_use]
    pub const fn trailing_newline(&self) -> bool {
        self.trailing_newline
    }
}

/// A lazy request for one exact file version.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceRequest {
    pub epoch: u64,
    pub key: SourceKey,
    /// Actual path at this version (not necessarily the current review path).
    pub source_path: RepoPath,
}

/// A host response containing exact UTF-8 source or a deterministic failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceResponse {
    pub epoch: u64,
    pub key: SourceKey,
    pub result: Result<Arc<str>, SourceUnavailable>,
}

/// Why a complete source version cannot be displayed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum SourceUnavailable {
    #[error("source version is absent")]
    Absent,
    #[error("source version is binary or not valid UTF-8")]
    Binary,
    #[error("source version is too large ({bytes} bytes)")]
    TooLarge { bytes: u64 },
    #[error("source version has too many lines ({lines})")]
    TooManyLines { lines: usize },
    #[error("source snapshot budget was exceeded")]
    SnapshotBudgetExceeded,
    #[error("repository changed while capturing the snapshot")]
    UnstableSnapshot,
    #[error("{0}")]
    Error(String),
}

/// Current viewer state for one source key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceStatus {
    Missing,
    Queued,
    Loaded,
    Unavailable(SourceUnavailable),
    Stale,
}

/// First inconsistency between patch metadata and a complete source version.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{side:?} source line {line_number} does not match the diff")]
pub struct SourceValidationError {
    pub side: DiffSide,
    pub line_number: usize,
    pub expected: Arc<str>,
    pub actual: Option<Arc<str>>,
}

/// Validates every numbered patch line and final-newline marker on one side.
///
/// # Errors
/// Returns the first source line that is missing or differs from the patch.
pub fn validate_file_version(
    file: &FileDiff,
    side: DiffSide,
    source: &FileVersionText,
) -> Result<(), SourceValidationError> {
    for line in file.hunks.iter().flat_map(|hunk| &hunk.lines) {
        let Some(line_number) = line.line_number(side) else {
            continue;
        };
        let actual = source.line(line_number);
        let final_numbered_line = line_number == source.line_count();
        // A final no-newline marker and a trailing source newline must disagree.
        let newline_mismatch = (line.no_newline && !final_numbered_line)
            || (final_numbered_line && line.no_newline == source.trailing_newline());
        if actual != Some(line.text.as_ref()) || newline_mismatch {
            return Err(SourceValidationError {
                side,
                line_number,
                expected: Arc::clone(&line.text),
                actual: source.line_arc(line_number).cloned(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileDiff;

    #[test]
    fn exact_identity_and_normalized_lines_are_independent() {
        let lf = FileVersionText::try_from_text("a\nb\n").unwrap();
        let crlf = FileVersionText::try_from_text("a\r\nb\r\n").unwrap();
        assert_eq!(lf.lines(), crlf.lines());
        assert_eq!(lf.sequence_id(), crlf.sequence_id());
        assert_ne!(lf.content_id(), crlf.content_id());
        assert_eq!(lf.line(1), Some("a"));
        assert_eq!(lf.line(2), Some("b"));
        assert_eq!(lf.line(3), None);
        assert!(lf.trailing_newline());
    }

    #[test]
    fn empty_and_blank_files_have_distinct_line_models() {
        assert_eq!(FileVersionText::try_from_text("").unwrap().line_count(), 0);
        let blank = FileVersionText::try_from_text("\n").unwrap();
        assert_eq!(blank.line_count(), 1);
        assert_eq!(blank.line(1), Some(""));
    }

    #[test]
    fn validates_patch_lines_on_each_side() {
        let file = FileDiff::from_texts("a.rs", "old\n", "new\n").unwrap();
        assert!(
            validate_file_version(
                &file,
                DiffSide::Old,
                &FileVersionText::try_from_text("old\n").unwrap(),
            )
            .is_ok()
        );
        assert!(
            validate_file_version(
                &file,
                DiffSide::New,
                &FileVersionText::try_from_text("new\n").unwrap(),
            )
            .is_ok()
        );
        let error = validate_file_version(
            &file,
            DiffSide::New,
            &FileVersionText::try_from_text("wrong\n").unwrap(),
        )
        .unwrap_err();
        assert_eq!(error.line_number, 1);
    }

    #[test]
    fn rejects_a_non_final_no_newline_marker() {
        let mut file = FileDiff::from_texts("a.rs", "old\n", "first\nsecond").unwrap();
        let line = file.hunks[0]
            .lines
            .iter_mut()
            .find(|line| line.new_line_no == Some(1))
            .unwrap();
        line.no_newline = true;
        let error = validate_file_version(
            &file,
            DiffSide::New,
            &FileVersionText::try_from_text("first\nsecond").unwrap(),
        )
        .unwrap_err();
        assert_eq!(error.line_number, 1);
    }
}
