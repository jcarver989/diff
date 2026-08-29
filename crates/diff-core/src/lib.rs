pub mod anchor;
pub mod error;
pub mod fingerprint;
pub mod highlight;
mod language;
pub mod markdown;
pub mod markdown_anchor;
pub mod markdown_review;
pub mod markdown_session;
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
pub use highlight::{HighlightStats, SyntaxHighlighter, empty_spans};
pub use markdown::{
    MarkdownBlock, MarkdownBlockKind, MarkdownCodeBlock, MarkdownCodeLine, MarkdownDocument,
    MarkdownHeading, MarkdownInline, MarkdownLineRange, MarkdownListItem, MarkdownTable,
    MarkdownTableAlignment, MarkdownTableCell, MarkdownTableRow, MarkdownTarget, MarkdownTargetId,
    MarkdownTargetKind, SourceRange, parse_markdown, rendered_text,
};
pub use markdown_anchor::{
    MarkdownAnchor, MarkdownBlockAnchor, MarkdownCodeLineAnchor, SNAPSHOT_CHAR_LIMIT,
};
pub use markdown_review::{
    MarkdownCommentContext, MarkdownReview, MarkdownReviewComment, MarkdownReviewDecision,
    MarkdownReviewError, MarkdownReviewEvent, MarkdownReviewSubmission, format_markdown_review,
};
pub use markdown_session::{MarkdownCommentDraft, MarkdownReviewSession};
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
    AgentFeedbackOptions, CommentContext, DiffReviewEvent, RepositoryAction, Review, ReviewComment,
    ReviewSubmission, format_review,
};
pub use session::{CommentDraft, ReviewSession, SessionOptions};
pub use theme::{
    DiffPalette, DiffTheme, FontStyle, HighlightSpan, Rgba, SyntaxStyle, ThemeError, ThemeId,
    ToneColors,
};
