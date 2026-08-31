//! Immutable complete-file source versions.

use crate::{DiffSide, Fingerprint, RepoPath, SourceSequenceId};
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

/// An immutable, contiguous source document with stable line coordinates.
///
/// The source text is retained as one allocation. `line_starts` makes line lookup
/// and byte spans constant time without copying or normalizing the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDocument {
    text: Arc<str>,
    line_starts: Arc<[usize]>,
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

impl SourceDocument {
    /// Captures one immutable source document.
    ///
    /// # Errors
    /// Returns a typed unavailable reason when the source exceeds the
    /// complete-file byte or line limits.
    pub fn new(text: impl AsRef<str>) -> Result<Self, SourceUnavailable> {
        Self::try_from_text(text.as_ref())
    }

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
        let trailing_newline = text.ends_with('\n');
        let mut line_starts = vec![0];
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(index + 1);
            }
        }

        if trailing_newline {
            line_starts.pop();
        }
        if text.is_empty() {
            line_starts.clear();
        }

        let line_count = line_starts.len();
        if line_count > MAX_SOURCE_FILE_LINES {
            return Err(SourceUnavailable::TooManyLines { lines: line_count });
        }
        let content_id = Fingerprint::of([b"diff-source-document-v1".as_slice(), text.as_bytes()]);
        let text: Arc<str> = Arc::from(text);
        let sequence_id = SourceSequenceId::from_lines((0..line_count).map(|line| {
            let start = line_starts[line];
            let end = text[start..]
                .find('\n')
                .map_or(text.len(), |offset| start + offset);
            text[start..end]
                .strip_suffix('\r')
                .unwrap_or(&text[start..end])
        }));
        Ok(Self {
            text,
            line_starts: line_starts.into(),
            content_id,
            sequence_id,
            byte_len,
            trailing_newline,
        })
    }

    /// Returns a one-based normalized source line.
    #[must_use]
    pub fn line(&self, number: usize) -> Option<&str> {
        let span = self.line_span(number)?;
        Some(&self.text[span])
    }

    /// Returns the zero-based byte span of a one-based source line.
    #[must_use]
    pub fn line_span(&self, number: usize) -> Option<std::ops::Range<usize>> {
        let index = number.checked_sub(1)?;
        let start = *self.line_starts.get(index)?;
        let end = self
            .line_starts
            .get(index + 1)
            .copied()
            .unwrap_or(self.text.len());
        let end = if end > start && self.text.as_bytes()[end - 1] == b'\n' {
            end - 1
        } else {
            end
        };
        let end = if end > start && self.text.as_bytes()[end - 1] == b'\r' {
            end - 1
        } else {
            end
        };
        Some(start..end)
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn line_starts(&self) -> &[usize] {
        &self.line_starts
    }

    #[must_use]
    pub const fn identity(&self) -> Fingerprint {
        self.content_id
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_identity_and_normalized_lines_are_independent() {
        let lf = SourceDocument::try_from_text("a\nb\n").unwrap();
        let crlf = SourceDocument::try_from_text("a\r\nb\r\n").unwrap();
        assert_eq!(lf.line(1), crlf.line(1));
        assert_eq!(lf.line(2), crlf.line(2));
        assert_eq!(lf.sequence_id(), crlf.sequence_id());
        assert_ne!(lf.content_id(), crlf.content_id());
        assert_eq!(lf.line(1), Some("a"));
        assert_eq!(lf.line(2), Some("b"));
        assert_eq!(lf.line(3), None);
        assert!(lf.trailing_newline());
    }

    #[test]
    fn contiguous_documents_provide_stable_byte_coordinates() {
        let document = SourceDocument::new("α\r\nb\n").unwrap();
        assert_eq!(document.text(), "α\r\nb\n");
        assert_eq!(document.line_starts(), &[0, 4]);
        assert_eq!(document.line_span(1), Some(0..2));
        assert_eq!(document.line_span(2), Some(4..5));
        assert_eq!(document.line(1), Some("α"));
        assert_eq!(document.identity(), document.content_id());
    }

    #[test]
    fn empty_and_blank_files_have_distinct_line_models() {
        assert_eq!(SourceDocument::try_from_text("").unwrap().line_count(), 0);
        let blank = SourceDocument::try_from_text("\n").unwrap();
        assert_eq!(blank.line_count(), 1);
        assert_eq!(blank.line(1), Some(""));
    }
}
