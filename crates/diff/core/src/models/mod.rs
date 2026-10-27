//! Owned, serializable diff domain types.

mod commit_stack;
mod diff_document;
mod diff_scope;
mod diff_side;
mod file_diff;
mod hunk;
mod patch_derivation;
mod patch_line;
mod repo_path;
mod review_target;

pub use commit_stack::{CommitStack, CommitSummary, LineStat};
pub use diff_document::DiffDocument;
pub use diff_scope::DiffScope;
pub use diff_side::DiffSide;
pub use file_diff::{FileDiff, FileStatus, ModeChange, StageState};
pub use hunk::Hunk;
pub use patch_line::{PatchLine, PatchLineKind};
pub use repo_path::RepoPath;
pub use review_target::{CommitId, ReviewTarget};
