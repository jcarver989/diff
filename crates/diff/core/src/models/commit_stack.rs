use super::{CommitId, DiffScope, ReviewTarget};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineStat {
    pub added: usize,
    pub deleted: usize,
    #[serde(default)]
    pub files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitSummary {
    pub id: CommitId,
    pub subject: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stat: Option<LineStat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitStack {
    pub base_ref: String,
    pub commits: Vec<CommitSummary>,
}

impl CommitStack {
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.commits.len() + 1
    }

    #[must_use]
    pub fn page_index(&self, target: &ReviewTarget) -> Option<usize> {
        match target {
            ReviewTarget::WorkingTree(_) => Some(self.commits.len()),
            ReviewTarget::Commit(id) => self.commits.iter().position(|commit| &commit.id == id),
        }
    }

    #[must_use]
    pub fn page(&self, index: usize, scope: DiffScope) -> Option<ReviewTarget> {
        match self.commits.get(index) {
            Some(commit) => Some(ReviewTarget::Commit(commit.id.clone())),
            None => (index == self.commits.len()).then_some(ReviewTarget::WorkingTree(scope)),
        }
    }
}
