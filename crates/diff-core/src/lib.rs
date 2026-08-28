//! Renderer-independent diff parsing, presentation, review, themes, and highlighting.

pub mod anchor;
pub mod error;
pub mod fingerprint;
pub mod highlight;
pub mod model;
pub mod parser;
pub mod presentation;
pub mod review;
pub mod session;
pub mod theme;

#[cfg(feature = "test-support")]
pub mod testing;

pub use anchor::LineAnchor;
pub use error::{DiffError, FingerprintError, ParseDiffScopeError, RepoPathError};
pub use fingerprint::Fingerprint;
pub use highlight::{HighlightStats, SyntaxHighlighter};
pub use model::{
    DiffDocument, DiffScope, DiffSide, FileDiff, FileStatus, Hunk, ModeChange, PatchLine,
    PatchLineKind, RepoPath, StageState,
};
pub use parser::{GitStatusEntry, UntrackedFile, parse_git_diff, parse_porcelain_v1_z};
pub use presentation::{
    DiffPresentation, DiffTone, Layout, PresentationOptions, PresentedCell, PresentedRow, RowId,
    RowKind, ViewMode,
};
pub use review::{
    AgentFeedbackOptions, CommentContext, DiffReviewEvent, Review, ReviewComment, ReviewSubmission,
    format_review,
};
pub use session::{CommentDraft, ReviewSession, SessionOptions};
pub use theme::{
    DiffPalette, DiffTheme, FontStyle, HighlightSpan, Rgba, ThemeError, ThemeId, ToneColors,
};
