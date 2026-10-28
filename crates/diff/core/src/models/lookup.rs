use super::{DiffDocument, DiffSide, RepoPath};
use crate::{Fingerprint, SourceDocument, SourceLocation};
use serde::{Deserialize, Serialize};
use std::{fmt, sync::Arc};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LookupRequest {
    Definition(DefinitionRequest),
    Source(SourcePath),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DefinitionRequest {
    pub at: SourceLocation,
    pub column: usize,
    pub content: Fingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Lookup {
    pub token: u64,
    pub request: LookupRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LookupAnswer {
    Definition(Resolution),
    Source(Arc<SourceDocument>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupReply {
    Definition { token: u64, request: DefinitionRequest, resolution: Resolution },
    Source { token: u64, path: SourcePath, result: Result<Arc<SourceDocument>, String> },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub targets: Vec<DefinitionTarget>,
    pub hover: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefinitionTarget {
    pub location: TargetLocation,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TargetLocation {
    pub path: SourcePath,
    pub side: DiffSide,
    pub line_number: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourcePath {
    Repo(RepoPath),
    External { id: u64, display: String },
}

impl DefinitionRequest {
    #[must_use]
    pub fn at(document: &DiffDocument, at: SourceLocation, column: usize) -> Option<Self> {
        if at.side != DiffSide::New {
            return None;
        }
        let content = document.source(&at.path, DiffSide::New)?.content_id();
        Some(Self { at, column, content })
    }
}

impl From<DefinitionRequest> for LookupRequest {
    fn from(request: DefinitionRequest) -> Self {
        Self::Definition(request)
    }
}

impl LookupReply {
    #[must_use]
    pub fn new(lookup: Lookup, result: Result<LookupAnswer, String>) -> Self {
        let token = lookup.token;
        match (lookup.request, result) {
            (LookupRequest::Definition(request), Ok(LookupAnswer::Definition(resolution))) => {
                Self::Definition { token, request, resolution }
            }
            (LookupRequest::Definition(request), _) => {
                Self::Definition { token, request, resolution: Resolution::default() }
            }
            (LookupRequest::Source(path), Ok(LookupAnswer::Source(document))) => {
                Self::Source { token, path, result: Ok(document) }
            }
            (LookupRequest::Source(path), Ok(LookupAnswer::Definition(_))) => {
                Self::Source { token, path, result: Err("the server sent no source".to_owned()) }
            }
            (LookupRequest::Source(path), Err(error)) => Self::Source { token, path, result: Err(error) },
        }
    }
}

impl From<SourceLocation> for TargetLocation {
    fn from(location: SourceLocation) -> Self {
        Self { path: SourcePath::Repo(location.path), side: location.side, line_number: location.line_number }
    }
}

impl SourcePath {
    #[must_use]
    pub fn display_path(&self) -> RepoPath {
        match self {
            Self::Repo(path) => path.clone(),
            Self::External { id, display } => RepoPath::new(display)
                .or_else(|_| RepoPath::new(format!("external-{id}")))
                .expect("a numbered external path is a valid repository path"),
        }
    }
}

impl fmt::Display for SourcePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repo(path) => write!(f, "{path}"),
            Self::External { display, .. } => f.write_str(display),
        }
    }
}

impl fmt::Display for TargetLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.path, self.line_number)
    }
}
