use crate::{
    server::{ServerEvent, ServerMessage},
    shared::{DocumentUpdate, FileEntry},
};
use clankerdiff_core::{DiffDocument, FileDiff, RepoPath};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, Default)]
pub struct SentDocument {
    files: BTreeMap<RepoPath, Arc<FileDiff>>,
}

impl SentDocument {
    pub fn encode_event(&mut self, event: ServerEvent) -> ServerMessage {
        event.map(|document| self.encode(&document))
    }

    pub fn encode(&mut self, document: &DiffDocument) -> DocumentUpdate {
        let mut files = Vec::with_capacity(document.files.len());
        let mut sent = BTreeMap::new();
        for file in &document.files {
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
            target: document.target.clone(),
            commits: document.commits.clone(),
            repo_root: document.repo_root.clone(),
            files,
        }
    }
}
