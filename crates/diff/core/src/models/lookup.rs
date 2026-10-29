use super::{DiffDocument, DiffSide, RepoPath};
use crate::{Fingerprint, SourceDocument, SourceLocation, SourceUnavailable};
use serde::{Deserialize, Serialize};
use std::{fmt, sync::Arc};
use thiserror::Error;

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
    #[serde(default)]
    pub status: LookupStatus,
    #[serde(default)]
    pub current_version: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LookupStatus {
    #[default]
    Complete,
    Unavailable(LookupUnavailable),
    Failed(LookupFailure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Error)]
#[serde(rename_all = "snake_case")]
pub enum LookupUnavailable {
    #[error("no language server handles this file")]
    NoLanguageServer,
    #[error("this line isn't in the current version, so it can't be looked up")]
    NotInCurrentVersion,
    #[error("the file on disk has changed since this version")]
    ChangedSinceReview,
    #[error("the file's full source isn't available")]
    SourceUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[serde(rename_all = "snake_case")]
pub enum LookupFailure {
    #[error("the language server didn't answer in time")]
    TimedOut,
    #[error("too many lookups are already running")]
    Busy,
    #[error("{0}")]
    Server(String),
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
    pub fn at(document: &DiffDocument, at: SourceLocation, column: usize) -> Result<Self, LookupUnavailable> {
        let file = document
            .file_index(&at.path)
            .and_then(|index| document.files.get(index))
            .ok_or(LookupUnavailable::SourceUnavailable)?;
        let new = match file.source(DiffSide::New) {
            Ok(new) => new,
            Err(SourceUnavailable::Absent) if at.side == DiffSide::Old => {
                return Err(LookupUnavailable::NotInCurrentVersion);
            }
            Err(_) => return Err(LookupUnavailable::SourceUnavailable),
        };
        let line_number = match at.side {
            DiffSide::New => at.line_number,
            DiffSide::Old => file
                .unchanged_new_line(at.line_number)
                .filter(|&line| new.line(line).is_some())
                .ok_or(LookupUnavailable::NotInCurrentVersion)?,
        };
        let at = SourceLocation { side: DiffSide::New, line_number, ..at };
        Ok(Self { at, column, content: new.content_id() })
    }
}

impl Resolution {
    #[must_use]
    pub const fn failed(failure: LookupFailure) -> Self {
        Self { targets: Vec::new(), hover: None, status: LookupStatus::Failed(failure), current_version: false }
    }

    #[must_use]
    pub const fn unavailable(reason: LookupUnavailable) -> Self {
        Self { targets: Vec::new(), hover: None, status: LookupStatus::Unavailable(reason), current_version: false }
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
            (LookupRequest::Definition(request), Ok(LookupAnswer::Source(_))) => {
                let resolution = Resolution::failed(LookupFailure::Server("the server sent a source".to_owned()));
                Self::Definition { token, request, resolution }
            }
            (LookupRequest::Definition(request), Err(error)) => {
                Self::Definition { token, request, resolution: Resolution::failed(LookupFailure::Server(error)) }
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
