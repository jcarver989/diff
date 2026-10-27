use super::{CommitStack, DiffScope};
use crate::CommitIdError;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const UNCOMMITTED_CHANGES: &str = "Uncommitted changes";

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
    pub fn label(&self, commits: Option<&CommitStack>) -> PageLabel {
        let page = commits.and_then(|stack| Some((stack, stack.page_index(self)?)));
        let kind = match self {
            Self::WorkingTree(scope) => PageKind::WorkingTree(*scope),
            Self::Commit(id) => PageKind::Commit {
                short_id: id.short().to_owned(),
                subject: page.map(|(stack, index)| stack.commits[index].subject.clone()),
            },
        };
        PageLabel { position: page.map(|(stack, index)| (index + 1, stack.page_count())), kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageKind {
    WorkingTree(DiffScope),
    Commit { short_id: String, subject: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageLabel {
    pub position: Option<(usize, usize)>,
    pub kind: PageKind,
}

impl PageLabel {
    #[must_use]
    pub fn is_paged(&self) -> bool {
        self.position.is_none_or(|(_, count)| count > 1)
    }

    #[must_use]
    pub fn short_id(&self) -> Option<&str> {
        match &self.kind {
            PageKind::WorkingTree(_) => None,
            PageKind::Commit { short_id, .. } => Some(short_id),
        }
    }

    #[must_use]
    pub fn subject(&self) -> Option<&str> {
        match &self.kind {
            PageKind::WorkingTree(_) => Some(UNCOMMITTED_CHANGES),
            PageKind::Commit { subject, .. } => subject.as_deref(),
        }
    }

    #[must_use]
    pub fn position_label(&self) -> Option<String> {
        self.position.map(|(number, count)| format!("{number}/{count}"))
    }

    #[must_use]
    pub fn heading(&self) -> String {
        match &self.kind {
            PageKind::WorkingTree(scope) => format!("{UNCOMMITTED_CHANGES} ({scope})"),
            PageKind::Commit { short_id, subject: Some(subject) } => format!("Commit {short_id} · {subject}"),
            PageKind::Commit { short_id, subject: None } => format!("Commit {short_id}"),
        }
    }
}

impl fmt::Display for PageLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some((number, count)) = self.position {
            write!(f, "{number}/{count} · ")?;
        }
        f.write_str(&self.heading())
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
