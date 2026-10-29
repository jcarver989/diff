use crate::server::Context;
use clankerdiff_core::{
    DefinitionRequest, DefinitionTarget, DiffDocument, DiffSide, LookupAnswer, LookupFailure, LookupRequest,
    LookupStatus, LookupUnavailable, RepoPath, Resolution, SourceDocument, SourcePath, SourceUnavailable,
    TargetLocation,
};
use clankerdiff_git::{GitRepository, read_source_file};
use clankerdiff_lspd::{ClientError, LanguageId, LspClient, WorkspaceKey, path_to_uri, uri_to_path};
use lsp_types::{Hover, HoverContents, Location, MarkedString};
use std::{
    collections::HashMap,
    env,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use thiserror::Error;
use tokio::{
    fs,
    task::JoinSet,
    time::{Instant, timeout_at},
};

const MAX_RUNNING: usize = 32;
const HOVER_GRACE: Duration = Duration::from_secs(1);
const RUST_LIBRARY: &str = "/lib/rustlib/src/rust/library/";
const CARGO_REGISTRY: &str = "/.cargo/registry/src/";
const NODE_MODULES: &str = "/node_modules/";
const GO_MODULES: &str = "/pkg/mod/";

#[derive(Debug, Error)]
pub(crate) enum LookupError {
    #[error("too many lookups are already running")]
    Busy,
    #[error("unknown external source {0}")]
    UnknownExternal(u64),
    #[error("{path} cannot be shown: {reason}")]
    Unavailable { path: String, reason: SourceUnavailable },
}

type Externals = Vec<(u64, PathBuf)>;

#[derive(Default)]
pub(crate) struct Lookups {
    running: JoinSet<(u64, Result<LookupAnswer, LookupError>, Externals)>,
    externals: HashMap<u64, PathBuf>,
    clients: HashMap<WorkspaceKey, LspClient>,
}

impl Lookups {
    pub(crate) fn submit(&mut self, context: &Context, id: u64, request: LookupRequest, snapshot: Arc<DiffDocument>) {
        let busy = self.running.len() >= MAX_RUNNING;
        let external = match &request {
            LookupRequest::Source(SourcePath::External { id, .. }) => self.externals.get(id).cloned(),
            _ => None,
        };
        let client = match &request {
            LookupRequest::Definition(definition) => {
                self.client(&context.repository.root().join(definition.at.path.as_str()))
            }
            LookupRequest::Source(_) => None,
        };
        let lookup =
            Request { repository: context.repository.clone(), client, snapshot, timeout: context.lookup_timeout };
        self.running.spawn(async move {
            match request {
                LookupRequest::Source(_) if busy => (id, Err(LookupError::Busy), Vec::new()),
                LookupRequest::Source(source) => (id, lookup.source(&source, external).await, Vec::new()),
                LookupRequest::Definition(_) if busy => {
                    (id, Ok(LookupAnswer::Definition(Resolution::failed(LookupFailure::Busy))), Vec::new())
                }
                LookupRequest::Definition(definition) => {
                    let (resolution, externals) = lookup.definition(&definition).await;
                    (id, Ok(LookupAnswer::Definition(resolution)), externals)
                }
            }
        });
    }

    fn client(&mut self, path: &Path) -> Option<LspClient> {
        let workspace = WorkspaceKey::resolve(path, LanguageId::from_path(path)?);
        Some(self.clients.entry(workspace).or_insert_with_key(|workspace| LspClient::new(workspace.clone())).clone())
    }

    pub(crate) async fn next(&mut self) -> Option<(u64, Result<LookupAnswer, LookupError>)> {
        let (id, answer, externals) = self.running.join_next().await?.ok()?;
        self.externals.extend(externals);
        Some((id, answer))
    }
}

struct Request {
    repository: GitRepository,
    client: Option<LspClient>,
    snapshot: Arc<DiffDocument>,
    timeout: Duration,
}

impl Request {
    async fn source(&self, source: &SourcePath, external: Option<PathBuf>) -> Result<LookupAnswer, LookupError> {
        let document = match (source, external) {
            (SourcePath::Repo(path), _) => self.repository.read_worktree_source(path).await,
            (SourcePath::External { id, .. }, None) => return Err(LookupError::UnknownExternal(*id)),
            (SourcePath::External { .. }, Some(path)) => read_source_file(&path).await,
        };
        document
            .map(LookupAnswer::Source)
            .map_err(|reason| LookupError::Unavailable { path: source.to_string(), reason })
    }

    async fn definition(&self, request: &DefinitionRequest) -> (Resolution, Externals) {
        let Some(client) = self.client.as_ref() else {
            return (Resolution::unavailable(LookupUnavailable::NoLanguageServer), Vec::new());
        };
        let at = &request.at;
        if at.side == DiffSide::Old {
            return (Resolution::unavailable(LookupUnavailable::NotInCurrentVersion), Vec::new());
        }
        let Some((source, line_number)) = self.worktree_line(request).await else {
            return (Resolution::unavailable(LookupUnavailable::ChangedSinceReview), Vec::new());
        };
        let Some((line, character)) = position(&source, line_number, request.column) else {
            return (Resolution::unavailable(LookupUnavailable::ChangedSinceReview), Vec::new());
        };
        let root = self.repository.root();
        let uri = match path_to_uri(&root.join(at.path.as_str())) {
            Ok(uri) => uri,
            Err(error) => return (Resolution::failed(LookupFailure::Server(error.to_string())), Vec::new()),
        };
        let deadline = Instant::now() + self.timeout;
        let definition = timeout_at(deadline, client.definition_locations(uri.clone(), line, character));
        let hover = timeout_at(deadline, client.hover(uri, line, character));
        tokio::pin!(definition, hover);
        let (locations, hover) = tokio::select! {
            locations = &mut definition => {
                let grace = (Instant::now() + HOVER_GRACE).min(deadline);
                (locations, timeout_at(grace, &mut hover).await.and_then(|hover| hover))
            }
            hover = &mut hover => (definition.await, hover),
        };
        let (locations, status) = match locations {
            Ok(Ok(locations)) => (locations, LookupStatus::Complete),
            Ok(Err(error)) => (Vec::new(), LookupStatus::Failed(failure(error))),
            Err(_) => (Vec::new(), LookupStatus::Failed(LookupFailure::TimedOut)),
        };
        let root = fs::canonicalize(root).await.unwrap_or_else(|_| root.to_path_buf());
        let mut sources = HashMap::new();
        let (mut targets, mut externals) = (Vec::new(), Vec::new());
        for location in locations {
            if let Some((target, external)) = self.definition_target(&root, &location, &mut sources).await {
                targets.push(target);
                externals.extend(external);
            }
        }
        let hover = hover.ok().and_then(Result::ok).flatten().and_then(hover_markdown);
        let current_version = source.content_id() != request.content;
        (Resolution { targets, hover, status, current_version }, externals)
    }

    async fn worktree_line(&self, request: &DefinitionRequest) -> Option<(Arc<SourceDocument>, usize)> {
        let at = &request.at;
        let source = self.repository.read_worktree_source(&at.path).await.ok()?;
        if source.content_id() == request.content {
            return Some((source, at.line_number));
        }
        let reviewed = self
            .snapshot
            .source(&at.path, DiffSide::New)
            .filter(|reviewed| reviewed.content_id() == request.content)?;
        let line_number = reviewed.matching_line(&source, at.line_number)?;
        Some((source, line_number))
    }

    async fn definition_target(
        &self,
        root: &Path,
        location: &Location,
        sources: &mut HashMap<PathBuf, Option<Arc<SourceDocument>>>,
    ) -> Option<(DefinitionTarget, Option<(u64, PathBuf)>)> {
        let path = uri_to_path(&location.uri);
        let line_number = usize::try_from(location.range.start.line).ok()? + 1;
        let repo_path = repository_path(root, &path);
        if !sources.contains_key(&path) {
            let source = match &repo_path {
                Some(repo_path) => self.repository.read_worktree_source(repo_path).await,
                None => read_source_file(&path).await,
            };
            sources.insert(path.clone(), source.ok());
        }
        let source = sources.get(&path).and_then(Option::as_ref);
        let preview = source
            .and_then(|source| source.line(line_number))
            .map(str::trim)
            .filter(|preview| !preview.is_empty())
            .map(ToOwned::to_owned);
        let (source, external) = if let Some(repo_path) = repo_path {
            if source.is_none_or(|source| !matches_snapshot(&self.snapshot, &repo_path, source)) {
                return None;
            }
            (SourcePath::Repo(repo_path), None)
        } else {
            let mut hasher = DefaultHasher::new();
            path.hash(&mut hasher);
            let id = hasher.finish();
            (SourcePath::External { id, display: external_display(&path) }, Some((id, path)))
        };
        let location = TargetLocation { path: source, side: DiffSide::New, line_number };
        Some((DefinitionTarget { location, preview }, external))
    }
}

fn repository_path(root: &Path, path: &Path) -> Option<RepoPath> {
    let relative = path.strip_prefix(root).ok()?;
    let parts: Vec<&str> = relative.components().map(|part| part.as_os_str().to_str()).collect::<Option<_>>()?;
    RepoPath::new(parts.join("/")).ok()
}

fn matches_snapshot(snapshot: &DiffDocument, path: &RepoPath, source: &SourceDocument) -> bool {
    snapshot.file_index(path).is_none_or(|_| {
        snapshot.source(path, DiffSide::New).is_some_and(|snapshot| snapshot.content_id() == source.content_id())
    })
}

fn position(source: &SourceDocument, line_number: usize, column: usize) -> Option<(u32, u32)> {
    let line_index = line_number.checked_sub(1)?;
    let line = source.line(line_number)?;
    let prefix = line.get(..column)?;
    let character = prefix.encode_utf16().count();
    Some((u32::try_from(line_index).ok()?, u32::try_from(character).ok()?))
}

fn failure(error: ClientError) -> LookupFailure {
    match error {
        ClientError::TimedOut { .. } => LookupFailure::TimedOut,
        error => LookupFailure::Server(error.to_string()),
    }
}

fn hover_markdown(hover: Hover) -> Option<String> {
    let marked = |value: MarkedString| match value {
        MarkedString::String(text) => text,
        MarkedString::LanguageString(code) => format!("```{}\n{}\n```", code.language, code.value),
    };
    let text = match hover.contents {
        HoverContents::Markup(markup) => markup.value,
        HoverContents::Scalar(value) => marked(value),
        HoverContents::Array(values) => values.into_iter().map(marked).collect::<Vec<_>>().join("\n\n"),
    };
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn external_display(path: &Path) -> String {
    let full = path.to_string_lossy();
    let after = |marker: &str| full.find(marker).map(|index| &full[index + marker.len()..]);
    if let Some(rest) = after(RUST_LIBRARY) {
        return format!("std · {rest}");
    }
    if let Some((_, rest)) = after(CARGO_REGISTRY).and_then(|rest| rest.split_once('/')) {
        return labelled(rest, 1);
    }
    if let Some(rest) = after(NODE_MODULES) {
        return labelled(rest, if rest.starts_with('@') { 2 } else { 1 });
    }
    if let Some(rest) = after(GO_MODULES) {
        let module = rest.split('/').position(|part| part.contains('@')).map_or(1, |index| index + 1);
        return labelled(rest, module);
    }
    let home = env::var("HOME").ok().filter(|home| !home.is_empty());
    match home.as_deref().and_then(|home| full.strip_prefix(home)) {
        Some(rest) => format!("~{rest}"),
        None => full.trim_start_matches('/').to_owned(),
    }
}

fn labelled(rest: &str, package_parts: usize) -> String {
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() <= package_parts {
        return rest.to_owned();
    }
    format!("{} · {}", parts[..package_parts].join("/"), parts[package_parts..].join("/"))
}
