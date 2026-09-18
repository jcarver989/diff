use clankerdiff_core::{DiffDocument, DiffScope, FileDiff, RepoPath};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffSnapshot {
    pub scope: DiffScope,
    pub document: Arc<DiffDocument>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileEntry {
    Unchanged(RepoPath),
    Changed(Arc<FileDiff>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentUpdate {
    pub scope: DiffScope,
    pub repo_root: String,
    pub files: Vec<FileEntry>,
}
