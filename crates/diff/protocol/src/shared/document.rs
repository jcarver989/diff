use clankerdiff_core::{CommitStack, FileDiff, RepoPath, ReviewTarget};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileEntry {
    Unchanged(RepoPath),
    Changed(Arc<FileDiff>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentUpdate {
    pub target: ReviewTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commits: Option<Arc<CommitStack>>,
    pub repo_root: String,
    pub files: Vec<FileEntry>,
}
