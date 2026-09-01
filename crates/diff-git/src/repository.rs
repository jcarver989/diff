//! Concrete native Git repository service.

use crate::{GitError, command, command::CatFileBatch, path};
use diff_core::{
    DiffDocument, DiffScope, DiffSide, DiffSnapshot, FileDiff, FileStatus, Fingerprint, RepoPath,
    SourceDocument, SourceKey, SourceUnavailable, UntrackedFile, parse_porcelain_v1_z,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
const STATUS_ARGS: [&str; 4] = ["status", "--porcelain=v1", "-z", "--untracked-files=all"];
pub use diff_core::MAX_SOURCE_FILE_BYTES;
pub const MAX_SOURCE_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_UNTRACKED_SNAPSHOT_BYTES: u64 = MAX_SOURCE_ARCHIVE_BYTES;

/// A patch document and its bounded immutable complete-file versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositorySnapshot {
    pub document: DiffDocument,
    sources: SourceArchive,
}

impl RepositorySnapshot {
    #[must_use]
    pub const fn sources(&self) -> &SourceArchive {
        &self.sources
    }

    #[must_use]
    pub fn into_parts(self) -> (DiffDocument, SourceArchive) {
        (self.document, self.sources)
    }

    /// Common immutable snapshot boundary consumed directly by native viewers.
    #[must_use]
    pub fn diff_snapshot(&self) -> DiffSnapshot {
        self.sources.snapshot(self.document.clone())
    }
}

/// Host-owned complete source documents, captured eagerly with patch metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceArchive {
    entries: HashMap<SourceKey, Result<Arc<SourceDocument>, SourceUnavailable>>,
}

impl SourceArchive {
    #[must_use]
    pub fn get(&self, key: &SourceKey) -> Option<&Result<Arc<SourceDocument>, SourceUnavailable>> {
        self.entries.get(key)
    }

    #[must_use]
    pub fn snapshot(&self, document: DiffDocument) -> DiffSnapshot {
        DiffSnapshot::new(document, self.entries.clone())
    }
}

fn document_from_captured_sources(
    mut document: DiffDocument,
    archive: &SourceArchive,
) -> DiffDocument {
    for file in &mut document.files {
        if file.binary {
            continue;
        }
        let source = |side| {
            let key = SourceKey::new(file.path.clone(), side);
            match archive.get(&key)? {
                Ok(document) => Some(document.text()),
                Err(SourceUnavailable::Absent)
                    if matches!(
                        (file.status, side),
                        (FileStatus::Added | FileStatus::Untracked, DiffSide::Old)
                            | (FileStatus::Deleted, DiffSide::New)
                    ) =>
                {
                    Some("")
                }
                Err(_) => None,
            }
        };
        let (Some(old), Some(new)) = (source(DiffSide::Old), source(DiffSide::New)) else {
            continue;
        };
        let Ok(mut derived) = FileDiff::from_texts(file.path.clone(), old, new) else {
            continue;
        };
        derived.old_path.clone_from(&file.old_path);
        derived.status = file.status;
        derived.staged = file.staged;
        derived.mode.clone_from(&file.mode);
        derived.omitted_bytes = file.omitted_bytes;
        *file = derived;
    }
    document
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ContentLocation {
    Absent,
    Head(RepoPath),
    Index(RepoPath),
    Worktree(RepoPath),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResolvedContentLocation {
    Absent,
    Blob(String),
    Worktree(RepoPath),
    Unavailable(SourceUnavailable),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SymlinkMode {
    CaptureLink,
    FollowContainedLink,
}

enum BoundedWorktree {
    Content(Vec<u8>),
    TooLarge(u64),
}

#[derive(Debug, Clone, Copy)]
enum BlobRecordKind {
    Tree,
    Index,
}

#[derive(Debug)]
struct SnapshotInput {
    has_head: bool,
    diff: Vec<u8>,
    status: Vec<u8>,
    document: DiffDocument,
}

#[derive(Debug)]
struct CapturedSources {
    archive: SourceArchive,
    worktree_ids: HashMap<RepoPath, Fingerprint>,
}

/// Bytes read from a worktree file, classified for safe text rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileContent {
    /// Valid UTF-8 content without embedded NUL bytes.
    Text(String),
    /// Binary or non-UTF-8 content.
    Binary(Vec<u8>),
}

impl FileContent {
    /// Classifies file bytes as renderer-safe text or binary content.
    #[must_use]
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        if bytes.contains(&0) {
            return Self::Binary(bytes);
        }
        match String::from_utf8(bytes) {
            Ok(text) => Self::Text(text),
            Err(error) => Self::Binary(error.into_bytes()),
        }
    }

    /// Returns the original file bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Text(text) => text.as_bytes(),
            Self::Binary(bytes) => bytes,
        }
    }

    /// Returns text content when this file was classified as text.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::Binary(_) => None,
        }
    }

    /// Returns whether the file was classified as binary.
    #[must_use]
    pub const fn is_binary(&self) -> bool {
        matches!(self, Self::Binary(_))
    }
}

/// A discovered Git worktree and its native operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRepository {
    root: PathBuf,
}

impl GitRepository {
    /// Discovers the containing Git worktree and stores its canonical root.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot run, the path is outside a worktree,
    /// or the discovered root cannot be represented safely.
    pub async fn discover(path: impl AsRef<Path>) -> Result<Self, GitError> {
        let candidate = path.as_ref();
        let output = match command::run_read(
            candidate,
            "discover repository",
            ["rev-parse", "--show-toplevel"],
        )
        .await
        {
            Ok(output) => output,
            Err(GitError::CommandFailed { .. }) => return Err(GitError::NotRepository),
            Err(error) => return Err(error),
        };
        let root = parse_root(&output.stdout)?;
        let root = tokio::fs::canonicalize(&root)
            .await
            .map_err(|source| GitError::Io {
                path: root.clone(),
                source,
            })?;
        Ok(Self { root })
    }

    /// Returns the canonical worktree root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Loads a canonical snapshot for a staged, unstaged, or combined scope.
    ///
    /// # Errors
    ///
    /// Returns an error when Git execution, path validation, file reading, or
    /// diff normalization fails.
    pub async fn snapshot(&self, scope: DiffScope) -> Result<DiffDocument, GitError> {
        self.load_snapshot_input(scope, scope == DiffScope::Both)
            .await
            .map(|input| input.document)
    }

    /// Captures patch metadata and exact bounded old/new source versions together.
    ///
    /// # Errors
    /// Returns an error when Git metadata cannot be loaded or captured source does not
    /// match the patch snapshot.
    pub async fn snapshot_with_sources(
        &self,
        scope: DiffScope,
    ) -> Result<RepositorySnapshot, GitError> {
        for _ in 0..2 {
            let initial = self.load_snapshot_input(scope, true).await?;
            let initial_locations = self
                .resolve_content_locations(&initial.document, scope, initial.has_head)
                .await;
            let captured = match self
                .capture_sources(&initial.document, &initial_locations)
                .await
            {
                Ok(captured) => captured,
                Err(GitError::UnstableSnapshot) => continue,
                Err(error) => return Err(error),
            };
            let final_input = self.load_snapshot_input(scope, true).await?;
            let final_locations = self
                .resolve_content_locations(&final_input.document, scope, final_input.has_head)
                .await;
            let metadata_stable = initial.has_head == final_input.has_head
                && initial.diff == final_input.diff
                && initial.status == final_input.status
                && initial.document == final_input.document
                && initial_locations == final_locations;
            if metadata_stable && self.worktrees_match(&captured.worktree_ids).await {
                let document = document_from_captured_sources(initial.document, &captured.archive);
                return Ok(RepositorySnapshot {
                    document,
                    sources: captured.archive,
                });
            }
        }
        Err(GitError::UnstableSnapshot)
    }

    async fn load_snapshot_input(
        &self,
        scope: DiffScope,
        resolve_head: bool,
    ) -> Result<SnapshotInput, GitError> {
        let has_head = if resolve_head {
            self.has_head().await?
        } else {
            true
        };
        let diff = command::run_read(&self.root, "load diff", Self::diff_args(scope, has_head))
            .await?
            .stdout;
        let status = command::run_read(&self.root, "load status", STATUS_ARGS)
            .await?
            .stdout;
        let untracked = if scope == DiffScope::Staged {
            Vec::new()
        } else {
            self.read_untracked().await?
        };
        let repo_root = self
            .root
            .to_str()
            .ok_or(GitError::UnsupportedRepositoryPath)?;
        let document = DiffDocument::from_git_outputs_with_untracked(
            repo_root, &diff, &status, scope, &untracked,
        )?;
        Ok(SnapshotInput {
            has_head,
            diff,
            status,
            document,
        })
    }

    async fn resolve_content_locations(
        &self,
        document: &DiffDocument,
        scope: DiffScope,
        has_head: bool,
    ) -> HashMap<SourceKey, ResolvedContentLocation> {
        let head = if has_head {
            self.resolve_head_blobs(document)
                .await
                .map_err(source_error)
        } else {
            Ok(HashMap::new())
        };
        let index = self.resolve_index_blobs().await.map_err(source_error);
        let mut resolved = HashMap::new();
        for file in &document.files {
            for side in [DiffSide::Old, DiffSide::New] {
                let key = SourceKey::new(file.path.clone(), side);
                let location = match content_location(scope, file, side, has_head) {
                    ContentLocation::Absent => ResolvedContentLocation::Absent,
                    ContentLocation::Worktree(path) => ResolvedContentLocation::Worktree(path),
                    ContentLocation::Head(path) => resolve_blob(&head, &path),
                    ContentLocation::Index(path) => resolve_blob(&index, &path),
                };
                resolved.insert(key, location);
            }
        }
        resolved
    }

    async fn capture_sources(
        &self,
        document: &DiffDocument,
        locations: &HashMap<SourceKey, ResolvedContentLocation>,
    ) -> Result<CapturedSources, GitError> {
        let mut archive = SourceArchive::default();
        let mut worktree_ids = HashMap::new();
        let mut loaded = 0_u64;
        let mut blobs = if locations
            .values()
            .any(|location| matches!(location, ResolvedContentLocation::Blob(_)))
        {
            Some(CatFileBatch::start(&self.root)?)
        } else {
            None
        };
        for file in &document.files {
            for side in [DiffSide::Old, DiffSide::New] {
                let key = SourceKey::new(file.path.clone(), side);
                let location = locations
                    .get(&key)
                    .cloned()
                    .unwrap_or(ResolvedContentLocation::Absent);
                let (result, exact_id) = if file.binary {
                    (Err(SourceUnavailable::Binary), None)
                } else {
                    self.capture_location(&location, &mut loaded, blobs.as_mut())
                        .await
                };
                if let (ResolvedContentLocation::Worktree(path), Some(exact_id)) =
                    (&location, exact_id)
                {
                    worktree_ids.insert(path.clone(), exact_id);
                }
                archive.entries.insert(key, result);
            }
        }
        Ok(CapturedSources {
            archive,
            worktree_ids,
        })
    }

    async fn capture_location(
        &self,
        location: &ResolvedContentLocation,
        loaded: &mut u64,
        blobs: Option<&mut CatFileBatch>,
    ) -> (
        Result<Arc<SourceDocument>, SourceUnavailable>,
        Option<Fingerprint>,
    ) {
        let bytes = match location {
            ResolvedContentLocation::Absent => return (Err(SourceUnavailable::Absent), None),
            ResolvedContentLocation::Unavailable(reason) => return (Err(reason.clone()), None),
            ResolvedContentLocation::Blob(oid) => {
                let Some(blobs) = blobs else {
                    return (
                        Err(SourceUnavailable::Error(
                            "source blob reader was not initialized".to_owned(),
                        )),
                        None,
                    );
                };
                match blobs.read_blob(oid, MAX_SOURCE_FILE_BYTES).await {
                    Ok(Ok(bytes)) => bytes,
                    Ok(Err(bytes)) => {
                        return (Err(SourceUnavailable::TooLarge { bytes }), None);
                    }
                    Err(error) => return (Err(source_error(error)), None),
                }
            }
            ResolvedContentLocation::Worktree(path) => match self.read_bounded_worktree(path).await
            {
                Ok(bytes) => bytes,
                Err(error) => return (Err(source_error(error)), None),
            },
        };
        let exact_id = matches!(location, ResolvedContentLocation::Worktree(_))
            .then(|| Fingerprint::of([bytes.as_slice()]));
        let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if size > MAX_SOURCE_FILE_BYTES {
            return (Err(SourceUnavailable::TooLarge { bytes: size }), exact_id);
        }
        if bytes.contains(&0) {
            return (Err(SourceUnavailable::Binary), exact_id);
        }
        let Ok(text) = String::from_utf8(bytes) else {
            return (Err(SourceUnavailable::Binary), exact_id);
        };
        if loaded.saturating_add(size) > MAX_SOURCE_ARCHIVE_BYTES {
            return (Err(SourceUnavailable::SnapshotBudgetExceeded), exact_id);
        }
        let source = match SourceDocument::new(&text) {
            Ok(source) => Arc::new(source),
            Err(reason) => return (Err(reason), exact_id),
        };
        *loaded = loaded.saturating_add(size);
        (Ok(source), exact_id)
    }

    async fn worktrees_match(&self, expected: &HashMap<RepoPath, Fingerprint>) -> bool {
        for (path, expected_id) in expected {
            let Ok(bytes) = self.read_bounded_worktree(path).await else {
                return false;
            };
            if Fingerprint::of([bytes.as_slice()]) != *expected_id {
                return false;
            }
        }
        true
    }

    async fn resolve_head_blobs(
        &self,
        document: &DiffDocument,
    ) -> Result<HashMap<RepoPath, String>, GitError> {
        let mut paths = document
            .files
            .iter()
            .flat_map(|file| {
                [
                    file.path.as_str(),
                    file.path_for_side(DiffSide::Old).as_str(),
                ]
            })
            .map(str::to_owned)
            .collect::<Vec<_>>();
        paths.sort_unstable();
        paths.dedup();
        let mut args = vec![
            "ls-tree".to_owned(),
            "-r".to_owned(),
            "-z".to_owned(),
            "HEAD".to_owned(),
            "--".to_owned(),
        ];
        args.extend(paths);
        let output = command::run_read(&self.root, "resolve HEAD sources", args).await?;
        Ok(parse_blob_records(&output.stdout, BlobRecordKind::Tree))
    }

    async fn resolve_index_blobs(&self) -> Result<HashMap<RepoPath, String>, GitError> {
        let output = command::run_read(
            &self.root,
            "resolve index sources",
            ["ls-files", "--stage", "-z"],
        )
        .await?;
        Ok(parse_blob_records(&output.stdout, BlobRecordKind::Index))
    }

    async fn read_bounded_worktree(&self, path: &RepoPath) -> Result<Vec<u8>, GitError> {
        match self
            .read_worktree_bytes(path, SymlinkMode::CaptureLink)
            .await?
        {
            BoundedWorktree::Content(bytes) => Ok(bytes),
            BoundedWorktree::TooLarge(bytes) => Err(GitError::SourceTooLarge { bytes }),
        }
    }

    async fn read_worktree_bytes(
        &self,
        path: &RepoPath,
        symlink_mode: SymlinkMode,
    ) -> Result<BoundedWorktree, GitError> {
        let joined = path::lexical_path(&self.root, path)?;
        let metadata = tokio::fs::symlink_metadata(&joined)
            .await
            .map_err(|source| GitError::Io {
                path: joined.clone(),
                source,
            })?;
        if metadata.file_type().is_symlink() && symlink_mode == SymlinkMode::CaptureLink {
            let target = tokio::fs::read_link(&joined)
                .await
                .map_err(|source| GitError::Io {
                    path: joined,
                    source,
                })?;
            return target
                .to_str()
                .map(|target| BoundedWorktree::Content(target.as_bytes().to_vec()))
                .ok_or(GitError::UnsupportedRepositoryPath);
        }
        let host_path = path::readable_path(&self.root, path).await?;
        let size = tokio::fs::metadata(&host_path)
            .await
            .map_err(|source| GitError::Io {
                path: host_path.clone(),
                source,
            })?
            .len();
        if size > MAX_SOURCE_FILE_BYTES {
            return Ok(BoundedWorktree::TooLarge(size));
        }
        tokio::fs::read(&host_path)
            .await
            .map(BoundedWorktree::Content)
            .map_err(|source| GitError::Io {
                path: host_path,
                source,
            })
    }

    /// Reads and classifies a complete worktree file.
    ///
    /// # Errors
    ///
    /// Returns an error when the path is invalid, missing, not a file, escapes
    /// through a symlink, or cannot be read.
    pub async fn read_worktree_file(&self, path: &RepoPath) -> Result<FileContent, GitError> {
        match self
            .read_worktree_bytes(path, SymlinkMode::FollowContainedLink)
            .await?
        {
            BoundedWorktree::Content(bytes) => Ok(FileContent::from_bytes(bytes)),
            BoundedWorktree::TooLarge(bytes) => Err(GitError::SourceTooLarge { bytes }),
        }
    }

    /// Stages selected paths. An empty slice is a no-op.
    ///
    /// # Errors
    ///
    /// Returns an error when a path is invalid or Git cannot stage it.
    pub async fn stage(&self, paths: &[RepoPath]) -> Result<(), GitError> {
        if paths.is_empty() {
            return Ok(());
        }
        self.run_paths("stage paths", &["add"], paths).await
    }

    /// Unstages selected paths while preserving their worktree contents.
    /// An empty slice is a no-op.
    ///
    /// # Errors
    ///
    /// Returns an error when a path is invalid or Git cannot update the index.
    pub async fn unstage(&self, paths: &[RepoPath]) -> Result<(), GitError> {
        if paths.is_empty() {
            return Ok(());
        }
        if self.has_head().await? {
            self.run_paths("unstage paths", &["reset", "--quiet", "HEAD"], paths)
                .await
        } else {
            self.run_paths(
                "unstage paths",
                &["rm", "--cached", "-f", "--quiet", "--ignore-unmatch"],
                paths,
            )
            .await
        }
    }

    /// Stages all tracked, untracked, and deleted paths.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot update the index.
    pub async fn stage_all(&self) -> Result<(), GitError> {
        command::run(&self.root, "stage all", ["add", "-A", "--"])
            .await
            .map(drop)
    }

    /// Unstages the complete index while preserving worktree contents.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot update the index.
    pub async fn unstage_all(&self) -> Result<(), GitError> {
        let args: &[&str] = if self.has_head().await? {
            &["reset", "--quiet", "HEAD", "--"]
        } else {
            &[
                "rm",
                "--cached",
                "-r",
                "-f",
                "--quiet",
                "--ignore-unmatch",
                "--",
                ".",
            ]
        };
        command::run(&self.root, "unstage all", args)
            .await
            .map(drop)
    }

    /// Commits the current index with the supplied non-empty message.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::EmptyCommitMessage`] for a blank message, or an
    /// execution error when Git cannot create the commit.
    pub async fn commit(&self, message: &str) -> Result<(), GitError> {
        if message.trim().is_empty() {
            return Err(GitError::EmptyCommitMessage);
        }
        command::run(&self.root, "commit", ["commit", "-m", message])
            .await
            .map(drop)
    }

    /// Discards all staged and unstaged changes for one path.
    ///
    /// Untracked paths are removed with `git clean`; tracked paths are restored
    /// from `HEAD`. In an unborn repository an added path is removed from the
    /// index and worktree.
    ///
    /// # Errors
    ///
    /// Returns an error when the path is invalid, status metadata cannot be
    /// parsed, or Git cannot restore or remove the path.
    pub async fn discard(&self, path: &RepoPath, status: FileStatus) -> Result<(), GitError> {
        path::lexical_path(&self.root, path)?;
        if status == FileStatus::Untracked {
            return self
                .run_paths(
                    "discard untracked path",
                    &["clean", "-f"],
                    std::slice::from_ref(path),
                )
                .await;
        }
        if !self.has_head().await? {
            if status != FileStatus::Added {
                return Err(GitError::CommandFailed {
                    operation: "discard path",
                    status: None,
                    stderr: "cannot restore a tracked path in a repository without HEAD".to_owned(),
                });
            }
            self.run_paths(
                "discard added path",
                &["rm", "--cached", "-f", "--ignore-unmatch"],
                std::slice::from_ref(path),
            )
            .await?;
            return self
                .run_paths(
                    "discard added path",
                    &["clean", "-f"],
                    std::slice::from_ref(path),
                )
                .await;
        }
        let mut restore_paths = vec![path.clone()];
        if status == FileStatus::Renamed {
            let output = command::run_read(&self.root, "resolve renamed path", STATUS_ARGS).await?;
            if let Some(old_path) = parse_porcelain_v1_z(&output.stdout)?
                .into_iter()
                .find(|entry| entry.path == *path)
                .and_then(|entry| entry.old_path)
            {
                restore_paths.push(old_path);
            }
        }
        self.run_paths(
            "discard path",
            &["restore", "--source=HEAD", "--staged", "--worktree"],
            &restore_paths,
        )
        .await
    }

    async fn read_untracked(&self) -> Result<Vec<UntrackedFile>, GitError> {
        let output = command::run_read(
            &self.root,
            "list untracked files",
            ["ls-files", "--others", "--exclude-standard", "-z", "--"],
        )
        .await?;
        let mut files = Vec::new();
        let mut loaded_bytes = 0_u64;
        for raw_path in output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
        {
            let text = std::str::from_utf8(raw_path)
                .map_err(diff_core::DiffError::UnsupportedPathEncoding)?;
            let path = RepoPath::new(text)?;
            let captured = self
                .read_worktree_bytes(&path, SymlinkMode::CaptureLink)
                .await?;
            let (contents, size) = match captured {
                BoundedWorktree::Content(contents) => {
                    let size = u64::try_from(contents.len()).unwrap_or(u64::MAX);
                    if loaded_bytes.saturating_add(size) > MAX_UNTRACKED_SNAPSHOT_BYTES {
                        (Vec::new(), size)
                    } else {
                        loaded_bytes = loaded_bytes.saturating_add(size);
                        (contents, size)
                    }
                }
                BoundedWorktree::TooLarge(size) => (Vec::new(), size),
            };
            let omitted = contents.is_empty() && size != 0;
            files.push(UntrackedFile {
                path,
                contents,
                omitted_bytes: omitted.then_some(size),
            });
        }
        Ok(files)
    }

    async fn has_head(&self) -> Result<bool, GitError> {
        match command::run_read(
            &self.root,
            "resolve HEAD",
            ["rev-parse", "--verify", "--quiet", "HEAD"],
        )
        .await
        {
            Ok(_) => Ok(true),
            Err(GitError::CommandFailed {
                status: Some(1), ..
            }) => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn diff_args(scope: DiffScope, has_head: bool) -> Vec<&'static str> {
        let mut args = vec![
            "diff",
            "--no-ext-diff",
            "--no-color",
            "--find-renames",
            "--find-copies",
            "--find-copies-harder",
        ];
        match scope {
            DiffScope::Unstaged => {}
            DiffScope::Staged => args.push("--cached"),
            DiffScope::Both => args.push(if has_head { "HEAD" } else { EMPTY_TREE }),
        }
        args.push("--");
        args
    }

    async fn run_paths<'a>(
        &self,
        operation: &'static str,
        prefix: &[&'a str],
        paths: &'a [RepoPath],
    ) -> Result<(), GitError> {
        let mut args = prefix.to_vec();
        args.push("--");
        for path in paths {
            path::lexical_path(&self.root, path)?;
            args.push(path.as_str());
        }
        command::run(&self.root, operation, args).await.map(drop)
    }
}

fn content_location(
    scope: DiffScope,
    file: &FileDiff,
    side: DiffSide,
    has_head: bool,
) -> ContentLocation {
    if side == DiffSide::Old && matches!(file.status, FileStatus::Added | FileStatus::Untracked) {
        return ContentLocation::Absent;
    }
    if side == DiffSide::New && file.status == FileStatus::Deleted {
        return ContentLocation::Absent;
    }
    let old_path = file.path_for_side(DiffSide::Old).clone();
    match (scope, side) {
        (DiffScope::Unstaged, DiffSide::Old) => ContentLocation::Index(old_path),
        (DiffScope::Unstaged | DiffScope::Both, DiffSide::New) => {
            ContentLocation::Worktree(file.path.clone())
        }
        (DiffScope::Staged | DiffScope::Both, DiffSide::Old) if has_head => {
            ContentLocation::Head(old_path)
        }
        (DiffScope::Staged | DiffScope::Both, DiffSide::Old) => ContentLocation::Absent,
        (DiffScope::Staged, DiffSide::New) => ContentLocation::Index(file.path.clone()),
    }
}

fn resolve_blob(
    blobs: &Result<HashMap<RepoPath, String>, SourceUnavailable>,
    path: &RepoPath,
) -> ResolvedContentLocation {
    match blobs {
        Ok(blobs) => blobs.get(path).cloned().map_or(
            ResolvedContentLocation::Absent,
            ResolvedContentLocation::Blob,
        ),
        Err(reason) => ResolvedContentLocation::Unavailable(reason.clone()),
    }
}

fn parse_blob_records(output: &[u8], kind: BlobRecordKind) -> HashMap<RepoPath, String> {
    output
        .split(|byte| *byte == 0)
        .filter_map(|record| {
            let tab = record.iter().position(|byte| *byte == b'\t')?;
            let header = std::str::from_utf8(&record[..tab]).ok()?;
            let path = std::str::from_utf8(&record[tab.saturating_add(1)..]).ok()?;
            let fields = header.split_ascii_whitespace().collect::<Vec<_>>();
            let oid = match kind {
                BlobRecordKind::Tree if fields.get(1) == Some(&"blob") => fields.get(2),
                BlobRecordKind::Index if fields.get(2) == Some(&"0") => fields.get(1),
                BlobRecordKind::Tree | BlobRecordKind::Index => None,
            }?;
            Some((RepoPath::new(path).ok()?, (*oid).to_owned()))
        })
        .collect()
}

fn source_error(error: GitError) -> SourceUnavailable {
    match error {
        GitError::SourceTooLarge { bytes } => SourceUnavailable::TooLarge { bytes },
        GitError::UnstableSnapshot => SourceUnavailable::UnstableSnapshot,
        other => SourceUnavailable::Error(other.to_string()),
    }
}

fn parse_root(stdout: &[u8]) -> Result<PathBuf, GitError> {
    let root = std::str::from_utf8(stdout)
        .map_err(|_| GitError::UnsupportedRepositoryPath)?
        .trim_end_matches(['\r', '\n']);
    if root.is_empty() {
        return Err(GitError::NotRepository);
    }
    Ok(PathBuf::from(root))
}
