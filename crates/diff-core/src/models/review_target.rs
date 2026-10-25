use super::{CommitStack, DiffScope};
use crate::CommitIdError;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CommitId(String);

impl CommitId {
    pub fn new(value: impl Into<String>) -> Result<Self, CommitIdError> {
        let value = value.into();
        if matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            Ok(Self(value.to_ascii_lowercase()))
        } else {
            Err(CommitIdError(value))
        }
    }

    #[must_use]
    pub fn short(&self) -> &str {
        &self.0[..7]
    }
}

impl TryFrom<String> for CommitId {
    type Error = CommitIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<CommitId> for String {
    fn from(value: CommitId) -> Self {
        value.0
    }
}

impl fmt::Display for CommitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewTarget {
    WorkingTree(DiffScope),
    Commit(CommitId),
}

impl ReviewTarget {
    #[must_use]
    pub const fn scope(&self) -> Option<DiffScope> {
        match self {
            Self::WorkingTree(scope) => Some(*scope),
            Self::Commit(_) => None,
        }
    }

    #[must_use]
    pub const fn commit(&self) -> Option<&CommitId> {
        match self {
            Self::WorkingTree(_) => None,
            Self::Commit(id) => Some(id),
        }
    }

    #[must_use]
    pub fn title(&self, commits: Option<&CommitStack>) -> String {
        let page = commits.and_then(|stack| Some((stack, stack.page_index(self)?)));
        let label = match (self, page) {
            (Self::WorkingTree(scope), _) => format!("Uncommitted changes ({scope})"),
            (Self::Commit(id), Some((stack, index))) => {
                format!("Commit {} · {}", id.short(), stack.commits[index].subject)
            }
            (Self::Commit(id), None) => format!("Commit {}", id.short()),
        };
        match page {
            Some((stack, index)) => format!("{}/{} · {label}", index + 1, stack.page_count()),
            None => label,
        }
    }
}

impl fmt::Display for ReviewTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkingTree(scope) => write!(f, "{scope}"),
            Self::Commit(id) => write!(f, "commit {}", id.short()),
        }
    }
}

impl Default for ReviewTarget {
    fn default() -> Self {
        Self::WorkingTree(DiffScope::default())
    }
}

impl From<DiffScope> for ReviewTarget {
    fn from(scope: DiffScope) -> Self {
        Self::WorkingTree(scope)
    }
}

impl From<CommitId> for ReviewTarget {
    fn from(id: CommitId) -> Self {
        Self::Commit(id)
    }
}
