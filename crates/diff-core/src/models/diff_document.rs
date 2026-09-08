use super::{FileDiff, RepoPath};
use crate::{DiffError, RepoPathError};
use serde::{Deserialize, Serialize};

/// A complete renderer-neutral review snapshot: every changed file with its
/// patch and, when captured, both complete source versions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffDocument {
    pub repo_root: String,
    pub files: Vec<FileDiff>,
}

impl DiffDocument {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            repo_root: String::new(),
            files: Vec::new(),
        }
    }

    /// Builds a complete document from full old/new text pairs, deriving
    pub fn from_texts<'a, T, U>(files: U) -> Result<Self, DiffError>
    where
        T: TryInto<RepoPath>,
        T::Error: Into<RepoPathError>,
        U: IntoIterator<Item = (T, &'a str, &'a str)>,
    {
        let files = files
            .into_iter()
            .map(|(path, old, new)| FileDiff::from_texts(path, old, new))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            repo_root: String::new(),
            files,
        })
    }

    #[must_use]
    pub fn file_index(&self, path: &RepoPath) -> Option<usize> {
        self.files.iter().position(|file| &file.path == path)
    }
}
