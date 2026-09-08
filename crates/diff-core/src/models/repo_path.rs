use crate::RepoPathError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, sync::Arc};

/// A validated UTF-8 path relative to a repository root.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RepoPath(Arc<str>);

impl RepoPath {
    /// Validates and stores a repository-relative UTF-8 path.
    ///
    /// # Errors
    ///
    /// Returns an error for empty, absolute, traversing, or NUL-containing paths.
    pub fn new(path: impl AsRef<str>) -> Result<Self, RepoPathError> {
        let raw = path.as_ref();
        if raw.is_empty() {
            return Err(RepoPathError::Empty);
        }
        if raw.bytes().any(|byte| byte == 0) {
            return Err(RepoPathError::Nul);
        }
        if raw.starts_with('/') || raw.starts_with("\\\\") || raw.as_bytes().get(1) == Some(&b':') {
            return Err(RepoPathError::Absolute);
        }
        for component in raw.split('/') {
            match component {
                "" => return Err(RepoPathError::Empty),
                "." | ".." => return Err(RepoPathError::Traversal),
                _ => {}
            }
        }
        Ok(Self(Arc::from(raw)))
    }

    /// Returns the validated Git path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl AsRef<str> for RepoPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<String> for RepoPath {
    type Error = RepoPathError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for RepoPath {
    type Error = RepoPathError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl Serialize for RepoPath {
    fn serialize<T: Serializer>(&self, serializer: T) -> Result<T::Ok, T::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RepoPath {
    fn deserialize<T: Deserializer<'de>>(deserializer: T) -> Result<Self, T::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
