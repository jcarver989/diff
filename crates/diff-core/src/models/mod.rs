//! Owned, serializable diff domain types.

mod diff_document;
mod diff_scope;
mod diff_side;
mod file_diff;
mod hunk;
mod patch_derivation;
mod patch_line;
mod repo_path;

pub use diff_document::DiffDocument;
pub use diff_scope::DiffScope;
pub use diff_side::DiffSide;
pub use file_diff::{FileDiff, FileStatus, ModeChange, StageState};
pub use hunk::Hunk;
pub use patch_line::{PatchLine, PatchLineKind};
pub use repo_path::RepoPath;
