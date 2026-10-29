//! Owned, serializable diff domain types.

mod commit_message;
mod commit_stack;
mod diff_document;
mod diff_scope;
mod diff_side;
mod file_diff;
mod hunk;
mod lookup;
mod patch_derivation;
mod patch_line;
mod repo_path;
mod review_target;

pub use commit_message::{CommitKind, CommitTitle, commit_description};
pub use commit_stack::{CommitStack, CommitSummary, LineStat};
pub use diff_document::DiffDocument;
pub use diff_scope::DiffScope;
pub use diff_side::DiffSide;
pub use file_diff::{FileDiff, FileStatus, ModeChange, StageState};
pub use hunk::Hunk;
pub use lookup::{
    DefinitionRequest, DefinitionTarget, Lookup, LookupAnswer, LookupFailure, LookupReply, LookupRequest, LookupStatus,
    LookupUnavailable, Resolution, SourcePath, TargetLocation,
};
pub use patch_line::{PatchLine, PatchLineKind};
pub use repo_path::RepoPath;
pub use review_target::{CommitId, PageKind, PageLabel, ReviewTarget, UNCOMMITTED_CHANGES};
