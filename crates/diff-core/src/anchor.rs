//! Stable identities for patch lines and review comments.

use crate::{DiffSide, FileDiff, PatchLine, PatchLineKind, RepoPath};
use serde::{Deserialize, Serialize};

/// A stable, serializable identity for a source line in a diff.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LineAnchor {
    pub path: RepoPath,
    pub side: DiffSide,
    pub old_line_no: Option<usize>,
    pub new_line_no: Option<usize>,
    pub kind: PatchLineKind,
    /// Lowercase hexadecimal BLAKE3 digest of path, content, and nearby context.
    pub fingerprint: String,
    /// Digest of side, kind, and line content, used when nearby context moves.
    pub content_fingerprint: String,
}
impl LineAnchor {
    /// Creates an anchor using up to two lines of context on either side.
    pub fn new(path: RepoPath, side: DiffSide, line: &PatchLine, nearby: &[&PatchLine]) -> Self {
        Self::from_context(path, side, line, nearby.iter().copied())
    }
    /// Creates an anchor from a file and hunk line index, collecting local context.
    pub fn for_line(file: &FileDiff, side: DiffSide, hunk: usize, line: usize) -> Option<Self> {
        let lines = &file.hunks.get(hunk)?.lines;
        let item = lines.get(line)?;
        item.line_number(side)?;
        let lo = line.saturating_sub(2);
        let hi = (line + 3).min(lines.len());
        Some(Self::from_context(
            file.path.clone(),
            side,
            item,
            lines[lo..hi].iter(),
        ))
    }
    fn from_context<'a>(
        path: RepoPath,
        side: DiffSide,
        line: &PatchLine,
        nearby: impl IntoIterator<Item = &'a PatchLine>,
    ) -> Self {
        let (fingerprint, content_fingerprint) = fingerprints(&path, side, line, nearby);
        Self {
            path,
            side,
            old_line_no: line.old_line_no,
            new_line_no: line.new_line_no,
            kind: line.kind,
            fingerprint,
            content_fingerprint,
        }
    }

    /// Returns the relevant line number for this side.
    pub fn line_number(&self) -> Option<usize> {
        match self.side {
            DiffSide::Old => self.old_line_no,
            DiffSide::New => self.new_line_no,
        }
    }
}
fn fingerprints<'a>(
    path: &RepoPath,
    side: DiffSide,
    line: &PatchLine,
    nearby: impl IntoIterator<Item = &'a PatchLine>,
) -> (String, String) {
    let side = match side {
        DiffSide::Old => "old",
        DiffSide::New => "new",
    };
    let kind = kind_name(line.kind);

    let mut stable = blake3::Hasher::new();
    for value in [path.as_str(), side, line.text.as_str()] {
        stable.update(value.as_bytes());
        stable.update(&[0]);
    }
    stable.update(kind.as_bytes());
    for context in nearby {
        stable.update(context.text.as_bytes());
        stable.update(&[0]);
    }

    let mut content = blake3::Hasher::new();
    content.update(side.as_bytes());
    content.update(kind.as_bytes());
    content.update(line.text.as_bytes());
    (
        stable.finalize().to_hex().to_string(),
        content.finalize().to_hex().to_string(),
    )
}

const fn kind_name(kind: PatchLineKind) -> &'static str {
    match kind {
        PatchLineKind::HunkHeader => "HunkHeader",
        PatchLineKind::Context => "Context",
        PatchLineKind::Added => "Added",
        PatchLineKind::Removed => "Removed",
        PatchLineKind::Meta => "Meta",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileDiff;
    #[test]
    fn deterministic_and_side_sensitive() {
        let f = FileDiff::from_texts("a.rs", "x\n", "y\n").unwrap();
        let a = LineAnchor::for_line(&f, DiffSide::Old, 0, 0).unwrap();
        let b = LineAnchor::for_line(&f, DiffSide::New, 0, 1).unwrap();
        assert_ne!(a.fingerprint, b.fingerprint);
        assert_eq!(a, LineAnchor::for_line(&f, DiffSide::Old, 0, 0).unwrap());
    }
}
