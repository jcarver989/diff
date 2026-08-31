pub mod anchor;
pub mod content;
pub mod error;
pub mod model;
pub mod parser;
pub mod presentation;
pub mod review;
pub mod session;

#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use anchor::LineAnchor;
pub use content::{
    FileVersionText, MAX_SOURCE_FILE_BYTES, MAX_SOURCE_FILE_LINES, SourceKey, SourceLineRef,
    SourceLocation, SourceRequest, SourceResponse, SourceStatus, SourceUnavailable,
    SourceValidationError, validate_file_version,
};
pub use diff_fingerprint::{Fingerprint, FingerprintError, SourceSequenceId};
pub use error::{DiffError, ParseDiffScopeError, RepoPathError};
pub use model::{
    DiffDocument, DiffScope, DiffSide, FileDiff, FileStatus, Hunk, ModeChange, PatchLine,
    PatchLineKind, RepoPath, StageState,
};
pub use parser::{GitStatusEntry, UntrackedFile, parse_git_diff, parse_porcelain_v1_z};
pub use presentation::{
    CellSequence, CellSequenceLines, ContentProjection, DiffPresentation, DiffTone, GapExpansion,
    GapId, GapInfo, GapInterval, Layout, PresentationOptions, PresentedCell, PresentedRow, RowId,
    RowKind, ViewMode, gaps_for_file,
};
pub use review::{
    AgentFeedbackOptions, CommentContext, DiffReviewEvent, RepositoryAction, Review, ReviewComment,
    ReviewSubmission, format_review,
};
pub use session::{CommentDraft, RevealAmount, ReviewSession, SessionOptions};
