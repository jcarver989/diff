use super::DiffSide;
use diff_theme::DiffTone;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A line's semantic patch kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PatchLineKind {
    HunkHeader,
    Context,
    Added,
    Removed,
    Meta,
}

impl PatchLineKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HunkHeader => "HunkHeader",
            Self::Context => "Context",
            Self::Added => "Added",
            Self::Removed => "Removed",
            Self::Meta => "Meta",
        }
    }

    #[must_use]
    pub const fn sides(self) -> &'static [DiffSide] {
        match self {
            Self::Removed => &[DiffSide::Old],
            Self::Added => &[DiffSide::New],
            _ => &[DiffSide::Old, DiffSide::New],
        }
    }

    /// Semantic tint used when presenting a line of this kind.
    #[must_use]
    pub const fn tone(self) -> DiffTone {
        match self {
            Self::Added => DiffTone::Added,
            Self::Removed => DiffTone::Removed,
            Self::Meta | Self::HunkHeader => DiffTone::Meta,
            Self::Context => DiffTone::Context,
        }
    }
}

/// A line in a hunk, including canonical old/new numbering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchLine {
    pub kind: PatchLineKind,
    pub text: Arc<str>,
    pub old_line_no: Option<usize>,
    pub new_line_no: Option<usize>,
    pub no_newline: bool,
}

impl PatchLine {
    /// Creates an added line.
    #[must_use]
    pub fn added(text: impl Into<Arc<str>>, new_line_no: usize) -> Self {
        Self {
            kind: PatchLineKind::Added,
            text: text.into(),
            old_line_no: None,
            new_line_no: Some(new_line_no),
            no_newline: false,
        }
    }

    /// Creates a removed line.
    #[must_use]
    pub fn removed(text: impl Into<Arc<str>>, old_line_no: usize) -> Self {
        Self {
            kind: PatchLineKind::Removed,
            text: text.into(),
            old_line_no: Some(old_line_no),
            new_line_no: None,
            no_newline: false,
        }
    }

    /// Creates a context line.
    #[must_use]
    pub fn context(text: impl Into<Arc<str>>, old_line_no: usize, new_line_no: usize) -> Self {
        Self {
            kind: PatchLineKind::Context,
            text: text.into(),
            old_line_no: Some(old_line_no),
            new_line_no: Some(new_line_no),
            no_newline: false,
        }
    }

    /// Marks this line as the final line without a trailing newline.
    #[must_use]
    pub fn without_newline(mut self) -> Self {
        self.no_newline = true;
        self
    }

    /// Returns the number for a side.
    #[must_use]
    pub const fn line_number(&self, side: DiffSide) -> Option<usize> {
        match side {
            DiffSide::Old => self.old_line_no,
            DiffSide::New => self.new_line_no,
        }
    }
}
