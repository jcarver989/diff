use crate::ParseDiffScopeError;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// The snapshot included in a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DiffScope {
    Unstaged,
    Staged,
    #[default]
    Both,
}

impl DiffScope {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unstaged => "unstaged",
            Self::Staged => "staged",
            Self::Both => "both",
        }
    }

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Unstaged => Self::Staged,
            Self::Staged => Self::Both,
            Self::Both => Self::Unstaged,
        }
    }
}

impl fmt::Display for DiffScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DiffScope {
    type Err = ParseDiffScopeError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "unstaged" => Ok(Self::Unstaged),
            "staged" => Ok(Self::Staged),
            "both" => Ok(Self::Both),
            other => Err(ParseDiffScopeError(other.to_owned())),
        }
    }
}
