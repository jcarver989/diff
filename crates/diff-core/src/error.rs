//! Errors returned by the renderer-independent diff model.

/// Errors produced while constructing or decoding a repository-relative path.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RepoPathError {
    /// The supplied path was empty.
    #[error("repository path is empty")]
    Empty,
    /// The path contained a NUL byte.
    #[error("repository path contains a NUL byte")]
    Nul,
    /// The path was absolute or contained a platform prefix.
    #[error("repository path must be relative")]
    Absolute,
    /// The path attempted to escape its repository root.
    #[error("repository path contains '.' or '..' component")]
    Traversal,
    /// A path supplied as bytes was not valid UTF-8.
    #[error("repository path is not valid UTF-8")]
    UnsupportedEncoding,
}

impl From<std::convert::Infallible> for RepoPathError {
    fn from(value: std::convert::Infallible) -> Self {
        match value {}
    }
}

/// Errors shared by model and parser adapters.
#[derive(Debug, thiserror::Error)]
pub enum DiffError {
    /// A repository path could not be represented safely.
    #[error("unsupported path encoding: {0}")]
    UnsupportedPathEncoding(#[source] std::str::Utf8Error),
    /// A path failed the relative-path contract.
    #[error("invalid repository path: {0}")]
    InvalidPath(#[from] RepoPathError),
    /// A diff could not be parsed.
    #[error("failed to parse diff: {source}")]
    Parse {
        /// Structured parser error, including the failing input span.
        #[source]
        source: diffy::patch_set::PatchSetParseError,
    },
    /// An underlying UTF-8 conversion failed.
    #[error("invalid UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    /// A NUL-delimited porcelain record did not follow Git's v1 grammar.
    #[error("invalid git porcelain v1 record")]
    InvalidPorcelainEntry,
}

impl From<std::string::FromUtf8Error> for DiffError {
    fn from(error: std::string::FromUtf8Error) -> Self {
        Self::UnsupportedPathEncoding(error.utf8_error())
    }
}

impl From<DiffError> for String {
    fn from(error: DiffError) -> Self {
        error.to_string()
    }
}
