pub mod anchor;
pub mod commands;
pub mod commit_picker;
pub mod content;
pub mod error;
pub mod file_tree;
pub mod keymap;
pub mod models;
pub mod parser;
pub mod patch;
pub mod presentation;
pub mod review;
pub mod session;
pub mod toolbar;

#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use anchor::LineAnchor;
pub use clankerdiff_fingerprint::{Fingerprint, FingerprintError, SourceSequenceId, join_lines};
pub use commands::{CommandContext, DiffReviewCommand, FocusPane, InteractionPhase, ReviewCapabilities, ReviewCommand};
pub use commit_picker::{CommitPage, CommitPicker, format_age};
pub use content::{
    MAX_SOURCE_FILE_BYTES, MAX_SOURCE_FILE_LINES, SourceDocument, SourceLineRef, SourceLocation, SourceResult,
    SourceUnavailable,
};
pub use error::{CommitIdError, DiffError, ParseDiffScopeError, RepoPathError};
pub use file_tree::{FileTree, FileTreeEntry, tree_order};
pub use keymap::{
    Chord, CommandEntry, CommandSection, DRAFT_HINT, Key, KeyScope, KeyState, browse_hint, command_entry,
    diff_commands, diff_shortcuts, join_key_labels, palette_commands,
};
pub use models as model;
pub use models::{
    CommitId, CommitKind, CommitStack, CommitSummary, CommitTitle, DefinitionRequest, DefinitionTarget, DiffDocument,
    DiffScope, DiffSide, FileDiff, FileStatus, Hunk, LineStat, Lookup, LookupAnswer, LookupReply, LookupRequest,
    ModeChange, PageKind, PageLabel, PatchLine, PatchLineKind, RepoPath, Resolution, ReviewTarget, SourcePath,
    StageState, TargetLocation, UNCOMMITTED_CHANGES, commit_description,
};
pub use parser::{
    GitStatusEntry, UntrackedFile, parse_detached_git_diff, parse_git_diff, parse_git_diff_with_path_mapper,
    parse_porcelain_v1_z,
};
pub use patch::{PatchError, git_patch_from_texts};
pub use presentation::{
    CellContext, ContentProjection, DiffPresentation, DiffTone, GapExpansion, GapId, GapInfo, GapInterval,
    HunkSequence, Layout, MAX_HUNK_SEQUENCE_LINES, PresentationOptions, PresentedCell, PresentedRow, RowId, RowKind,
    ViewMode, ViewportAnchor, gaps_for_file,
};
pub use review::{
    AgentFeedbackOptions, CommentContext, DiffReviewEvent, RepositoryAction, Review, ReviewComment, ReviewSubmission,
    format_review,
};
pub use session::{CommentDraft, RevealAmount, ReviewSession};
pub use toolbar::{ControlKind, Toolbar, ToolbarControl, default_key_label, tooltip};
