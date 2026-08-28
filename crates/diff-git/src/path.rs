//! Safe conversion from repository paths to host filesystem paths.

use crate::GitError;
use diff_core::RepoPath;
use std::path::{Component, Path, PathBuf};

pub(crate) fn lexical_path(root: &Path, path: &RepoPath) -> Result<PathBuf, GitError> {
    let relative = Path::new(path.as_str());
    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(GitError::PathEscapesRepository {
            path: path.to_string(),
        });
    }
    Ok(root.join(relative))
}

pub(crate) async fn readable_path(root: &Path, path: &RepoPath) -> Result<PathBuf, GitError> {
    let joined = lexical_path(root, path)?;
    let resolved = tokio::fs::canonicalize(&joined)
        .await
        .map_err(|source| GitError::Io {
            path: joined.clone(),
            source,
        })?;
    if !resolved.starts_with(root) {
        return Err(GitError::PathEscapesRepository {
            path: path.to_string(),
        });
    }
    let metadata = tokio::fs::metadata(&resolved)
        .await
        .map_err(|source| GitError::Io {
            path: resolved.clone(),
            source,
        })?;
    if !metadata.is_file() {
        return Err(GitError::NotAFile {
            path: path.to_string(),
        });
    }
    Ok(resolved)
}
