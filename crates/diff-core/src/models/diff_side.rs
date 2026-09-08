use serde::{Deserialize, Serialize};

/// The side of a patch to which a line or comment belongs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum DiffSide {
    Old,
    New,
}

impl DiffSide {
    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::Old => Self::New,
            Self::New => Self::Old,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Old => "old",
            Self::New => "new",
        }
    }
}
