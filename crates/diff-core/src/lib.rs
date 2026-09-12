pub mod anchor;
pub mod commands;
pub mod content;
pub mod error;
pub mod models;
pub mod parser;
pub mod patch;
pub mod presentation;
pub mod review;
pub mod session;

#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use anchor::LineAnchor;
pub use clankerdiff_fingerprint::{Fingerprint, FingerprintError, SourceSequenceId, join_lines};
pub use commands::{
    CommandContext, DiffReviewCommand, FocusPane, InteractionPhase, ReviewCapabilities,
    ReviewCommand,
};
pub use content::{
    MAX_SOURCE_FILE_BYTES, MAX_SOURCE_FILE_LINES, SourceDocument, SourceLineRef, SourceLocation,
    SourceResult, SourceUnavailable,
};
pub use error::{DiffError, ParseDiffScopeError, RepoPathError};
pub use models as model;
pub use models::{
    DiffDocument, DiffScope, DiffSide, FileDiff, FileStatus, Hunk, ModeChange, PatchLine,
    PatchLineKind, RepoPath, StageState,
};
pub use parser::{GitStatusEntry, UntrackedFile, parse_git_diff, parse_porcelain_v1_z};
pub use patch::{PatchError, git_patch_from_texts};
pub use presentation::{
    CellContext, ContentProjection, DiffPresentation, DiffTone, GapExpansion, GapId, GapInfo,
    GapInterval, HunkSequence, Layout, MAX_HUNK_SEQUENCE_LINES, PresentationOptions, PresentedCell,
    PresentedRow, RowId, RowKind, ViewMode, gaps_for_file,
};
pub use review::{
    AgentFeedbackOptions, CommentContext, DiffReviewEvent, RepositoryAction, Review, ReviewComment,
    ReviewSubmission, format_review,
};
pub use session::{CommentDraft, RevealAmount, ReviewSession, SessionOptions};
