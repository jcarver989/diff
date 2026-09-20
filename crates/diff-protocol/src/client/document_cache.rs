use crate::{
    server::{ServerEvent, ServerMessage},
    shared::{DiffSnapshot, DocumentUpdate, FileEntry, ProtocolError},
};
use clankerdiff_core::{DiffDocument, RepoPath};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, Default)]
pub struct DocumentCache {
    document: Arc<DiffDocument>,
    index: BTreeMap<RepoPath, usize>,
}

impl DocumentCache {
    pub fn decode_event(&mut self, text: &str) -> Result<ServerEvent, ProtocolError> {
        self.apply_event(ServerMessage::decode(text)?)
    }

    pub fn apply_event(&mut self, message: ServerMessage) -> Result<ServerEvent, ProtocolError> {
        message.try_map(|update| Ok(Arc::new(self.apply(&update)?)))
    }

    pub fn apply(&mut self, update: &DocumentUpdate) -> Result<DiffSnapshot, ProtocolError> {
        let mut files = Vec::with_capacity(update.files.len());
        let mut index = BTreeMap::new();
        for entry in &update.files {
            let file = match entry {
                FileEntry::Unchanged(path) => self
                    .index
                    .get(path)
                    .and_then(|index| self.document.files.get(*index))
                    .ok_or(ProtocolError::Files)?
                    .clone(),
                FileEntry::Changed(file) => file.as_ref().clone(),
            };
            if index.insert(file.path.clone(), files.len()).is_some() {
                return Err(ProtocolError::Files);
            }
            files.push(file);
        }
        self.document = Arc::new(DiffDocument {
            repo_root: update.repo_root.clone(),
            files,
        });
        self.index = index;
        Ok(DiffSnapshot {
            scope: update.scope,
            document: Arc::clone(&self.document),
        })
    }
}
