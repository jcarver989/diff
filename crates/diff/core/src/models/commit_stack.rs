use super::{CommitId, DiffScope, ReviewTarget};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitSummary {
    pub id: CommitId,
    pub subject: String,
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
