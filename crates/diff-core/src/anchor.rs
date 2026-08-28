//! Stable identities for patch lines and review comments.

use crate::{DiffSide, FileDiff, Fingerprint, PatchLine, PatchLineKind, RepoPath};
use serde::{Deserialize, Serialize};

const CONTEXT_RADIUS: usize = 2;

/// A stable, serializable identity for a source line in a diff.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LineAnchor {
    pub path: RepoPath,
    pub side: DiffSide,
    pub old_line_no: Option<usize>,
    pub new_line_no: Option<usize>,
    pub kind: PatchLineKind,
    pub fingerprint: Fingerprint,
    /// Digest of side, kind, and line content, used when nearby context moves.
    pub content_fingerprint: Fingerprint,
}

impl LineAnchor {
    /// Creates an anchor from a file and hunk line index, collecting local context.
    #[must_use]
    pub fn for_line(file: &FileDiff, side: DiffSide, hunk: usize, line: usize) -> Option<Self> {
        let lines = &file.hunks.get(hunk)?.lines;
        let anchored = lines.get(line)?;
        anchored.line_number(side)?;
        let start = line.saturating_sub(CONTEXT_RADIUS);
        let end = (line + CONTEXT_RADIUS + 1).min(lines.len());
        Some(Self {
            path: file.path.clone(),
            side,
            old_line_no: anchored.old_line_no,
            new_line_no: anchored.new_line_no,
            kind: anchored.kind,
            fingerprint: context_fingerprint(&file.path, side, anchored, &lines[start..end]),
            content_fingerprint: Self::content_fingerprint_of(side, anchored),
        })
    }

    #[must_use]
    pub fn content_fingerprint_of(side: DiffSide, line: &PatchLine) -> Fingerprint {
        Fingerprint::of([side.as_str(), line.kind.as_str(), line.text.as_ref()])
    }

    /// Returns the relevant line number for this side.
    #[must_use]
    pub const fn line_number(&self) -> Option<usize> {
        match self.side {
            DiffSide::Old => self.old_line_no,
            DiffSide::New => self.new_line_no,
        }
    }

    #[must_use]
    pub fn addresses_same_side(&self, other: &Self) -> bool {
        self.path == other.path && self.side == other.side
    }
}

fn context_fingerprint(
    path: &RepoPath,
    side: DiffSide,
    line: &PatchLine,
    nearby: &[PatchLine],
) -> Fingerprint {
    Fingerprint::of(
        [
            path.as_str(),
            side.as_str(),
            line.text.as_ref(),
            line.kind.as_str(),
        ]
        .into_iter()
        .chain(nearby.iter().map(|context| context.text.as_ref())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileDiff;

    #[test]
    fn deterministic_and_side_sensitive() {
        let file = FileDiff::from_texts("a.rs", "x\n", "y\n").unwrap();
        let old = LineAnchor::for_line(&file, DiffSide::Old, 0, 0).unwrap();
        let new = LineAnchor::for_line(&file, DiffSide::New, 0, 1).unwrap();
        assert_ne!(old.fingerprint, new.fingerprint);
        assert_eq!(
            old,
            LineAnchor::for_line(&file, DiffSide::Old, 0, 0).unwrap()
        );
        assert!(!old.addresses_same_side(&new));
    }

    #[test]
    fn cheap_content_digest_matches_the_stored_one() {
        let file = FileDiff::from_texts("a.rs", "x\n", "y\n").unwrap();
        let anchor = LineAnchor::for_line(&file, DiffSide::New, 0, 1).unwrap();
        let line = file.line(0, 1).unwrap();
        assert_eq!(
            anchor.content_fingerprint,
            LineAnchor::content_fingerprint_of(DiffSide::New, line)
        );
    }

    #[test]
    fn rejects_lines_without_a_number_on_the_requested_side() {
        let file = FileDiff::from_texts("a.rs", "x\n", "y\n").unwrap();
        assert!(LineAnchor::for_line(&file, DiffSide::Old, 0, 1).is_none());
        assert!(LineAnchor::for_line(&file, DiffSide::New, 9, 0).is_none());
    }
}
