use super::{PatchLine, PatchLineKind};
use serde::{Deserialize, Serialize};

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
