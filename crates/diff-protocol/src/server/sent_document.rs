use crate::shared::{DiffSnapshot, DocumentUpdate, FileEntry};
use clankerdiff_core::{FileDiff, RepoPath};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, Default)]
pub struct SentDocument {
    files: BTreeMap<RepoPath, Arc<FileDiff>>,
}

impl SentDocument {
    pub fn encode(&mut self, snapshot: &DiffSnapshot) -> DocumentUpdate {
        let mut files = Vec::with_capacity(snapshot.document.files.len());
        let mut sent = BTreeMap::new();
        for file in &snapshot.document.files {
            let entry = match self.files.get(&file.path) {
                Some(held) if held.as_ref() == file => {
                    sent.insert(file.path.clone(), Arc::clone(held));
                    FileEntry::Unchanged(file.path.clone())
                }
                _ => {
                    let file = Arc::new(file.clone());
                    sent.insert(file.path.clone(), Arc::clone(&file));
                    FileEntry::Changed(file)
                }
            };
            files.push(entry);
        }
        self.files = sent;
        DocumentUpdate {
            scope: snapshot.scope,
            repo_root: snapshot.document.repo_root.clone(),
            files,
        }
    }
}
