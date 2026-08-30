pub mod anchor;
pub mod error;
pub mod model;
pub mod parser;
pub mod presentation;
pub mod review;
pub mod session;

#[cfg(feature = "test-support")]
pub mod testing;

pub use anchor::LineAnchor;
pub use diff_fingerprint::{Fingerprint, FingerprintError};
pub use error::{DiffError, ParseDiffScopeError, RepoPathError};
pub use model::{
    DiffDocument, DiffScope, DiffSide, FileDiff, FileStatus, Hunk, ModeChange, PatchLine,
    PatchLineKind, RepoPath, StageState,
};
pub use parser::{GitStatusEntry, UntrackedFile, parse_git_diff, parse_porcelain_v1_z};
pub use presentation::{
    CellSequence, DiffPresentation, DiffTone, Layout, PresentationOptions, PresentedCell,
    PresentedRow, RowId, RowKind, SequenceId, ViewMode,
};
pub use review::{
    AgentFeedbackOptions, CommentContext, DiffReviewEvent, RepositoryAction, Review, ReviewComment,
    ReviewSubmission, format_review,
};
pub use session::{CommentDraft, ReviewSession, SessionOptions};
