//! Concrete native Git repository service.

use crate::{GitError, command, command::CatFileBatch, path};
use clankerdiff_core::{
    DiffDocument, DiffScope, DiffSide, FileDiff, FileStatus, Fingerprint, RepoPath,
    RepositoryAction, SourceDocument, SourceResult, SourceUnavailable, UntrackedFile,
    parse_porcelain_v1_z,
};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::time::sleep;

const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
const STATUS_ARGS: [&str; 5] = [
    "--no-optional-locks",
    "status",
    "--porcelain=v1",
    "-z",
    "--untracked-files=all",
];
pub use clankerdiff_core::MAX_SOURCE_FILE_BYTES;
pub const MAX_SOURCE_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_UNTRACKED_SNAPSHOT_BYTES: u64 = MAX_SOURCE_ARCHIVE_BYTES;

/// A complete review document captured for one scope, with bounded immutable
/// complete-file versions attached to every file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepositorySnapshot {
    pub scope: DiffScope,
    pub document: Arc<DiffDocument>,
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
    /// Old and new results for each file, in document order.
    sides: Vec<[SourceResult; 2]>,
    worktree_ids: HashMap<RepoPath, Fingerprint>,
}

/// A discovered Git worktree and its native operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRepository {
    root: PathBuf,
}

impl GitRepository {
    pub async fn apply(&self, action: RepositoryAction) -> Result<(), GitError> {
        match action {
            RepositoryAction::StagePaths(paths) => self.stage(&paths).await,
            RepositoryAction::UnstagePaths(paths) => self.unstage(&paths).await,
            RepositoryAction::StageAll => self.stage_all().await,
            RepositoryAction::UnstageAll => self.unstage_all().await,
            RepositoryAction::Commit { message } => self.commit(&message).await,
            RepositoryAction::Discard { path, status } => self.discard(&path, status).await,
        }
    }

    pub async fn discover(path: impl AsRef<Path>) -> Result<Self, GitError> {
        let candidate = path.as_ref();
        let output = match command::run(
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

    pub async fn metadata_directories(&self) -> Result<Vec<PathBuf>, GitError> {
        let mut directories = Vec::new();
        for argument in ["--git-dir", "--git-common-dir"] {
            let output = command::run(
                &self.root,
                "resolve Git metadata directory",
                ["rev-parse", "--path-format=absolute", argument],
            )
            .await?;
            let path = parse_root(&output.stdout)?;
            let directory = tokio::fs::canonicalize(&path)
                .await
                .map_err(|source| GitError::Io { path, source })?;
            if !directories.contains(&directory) {
                directories.push(directory);
            }
        }
        Ok(directories)
    }

    pub async fn snapshot(&self, scope: DiffScope) -> Result<DiffDocument, GitError> {
        self.load_snapshot_input(scope, scope == DiffScope::Both)
            .await
            .map(|input| input.document)
    }

    pub async fn snapshot_with_sources(
        &self,
        scope: DiffScope,
    ) -> Result<RepositorySnapshot, GitError> {
        let mut retries = 0;
        let mut delay = Duration::from_millis(250);
        loop {
            match self.capture_snapshot_with_sources(scope).await {
                Err(GitError::UnstableSnapshot) if retries < 5 => {
                    retries += 1;
                    sleep(delay).await;
                    delay = delay.saturating_mul(2).min(Duration::from_secs(2));
                }
                result => return result,
            }
        }
    }

    async fn capture_snapshot_with_sources(
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
                let files = initial
                    .document
                    .files
                    .into_iter()
                    .zip(captured.sides)
                    .map(|(file, [old, new])| file.with_sources(old, new))
                    .collect();
                return Ok(RepositorySnapshot {
                    scope,
                    document: Arc::new(DiffDocument {
                        repo_root: initial.document.repo_root,
                        files,
                    }),
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
        let diff = command::run(&self.root, "load diff", Self::diff_args(scope, has_head))
            .await?
            .stdout;
        let status = command::run(&self.root, "load status", STATUS_ARGS)
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

    /// Resolves where each file's old and new versions live, in document order.
    async fn resolve_content_locations(
        &self,
        document: &DiffDocument,
        scope: DiffScope,
        has_head: bool,
    ) -> Vec<[ResolvedContentLocation; 2]> {
        let head = if has_head {
            self.resolve_head_blobs(document)
                .await
                .map_err(source_error)
        } else {
            Ok(HashMap::new())
        };
        let index = self.resolve_index_blobs().await.map_err(source_error);
        document
            .files
            .iter()
            .map(|file| {
                [DiffSide::Old, DiffSide::New].map(|side| {
                    match content_location(scope, file, side, has_head) {
                        ContentLocation::Absent => ResolvedContentLocation::Absent,
                        ContentLocation::Worktree(path) => ResolvedContentLocation::Worktree(path),
                        ContentLocation::Head(path) => resolve_blob(&head, &path),
                        ContentLocation::Index(path) => resolve_blob(&index, &path),
                    }
                })
            })
            .collect()
    }

    async fn capture_sources(
        &self,
        document: &DiffDocument,
        locations: &[[ResolvedContentLocation; 2]],
    ) -> Result<CapturedSources, GitError> {
        let mut sides = Vec::with_capacity(document.files.len());
        let mut worktree_ids = HashMap::new();
        let mut loaded = 0_u64;
        let mut blobs = if locations
            .iter()
            .flatten()
            .any(|location| matches!(location, ResolvedContentLocation::Blob(_)))
        {
            Some(CatFileBatch::start(&self.root)?)
        } else {
            None
        };
        for (file, file_locations) in document.files.iter().zip(locations) {
            let mut results = Vec::with_capacity(2);
            for location in file_locations {
                let (result, exact_id) = if file.binary {
                    (Err(SourceUnavailable::Binary), None)
                } else {
                    self.capture_location(location, &mut loaded, blobs.as_mut())
                        .await
                };
                if let (ResolvedContentLocation::Worktree(path), Some(exact_id)) =
                    (location, exact_id)
                {
                    worktree_ids.insert(path.clone(), exact_id);
                }
                results.push(result);
            }
            let [old, new] = <[SourceResult; 2]>::try_from(results)
                .unwrap_or_else(|_| unreachable!("two sides per file"));
            sides.push([old, new]);
        }
        Ok(CapturedSources {
            sides,
            worktree_ids,
        })
    }

    async fn capture_location(
        &self,
        location: &ResolvedContentLocation,
        loaded: &mut u64,
        blobs: Option<&mut CatFileBatch>,
    ) -> (SourceResult, Option<Fingerprint>) {
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
        let output = command::run(&self.root, "resolve HEAD sources", args).await?;
        Ok(parse_blob_records(&output.stdout, BlobRecordKind::Tree))
    }

    async fn resolve_index_blobs(&self) -> Result<HashMap<RepoPath, String>, GitError> {
        let output = command::run(
            &self.root,
            "resolve index sources",
            ["ls-files", "--stage", "-z"],
        )
        .await?;
        Ok(parse_blob_records(&output.stdout, BlobRecordKind::Index))
    }

    async fn read_bounded_worktree(&self, path: &RepoPath) -> Result<Vec<u8>, GitError> {
        match self.read_worktree_bytes(path).await? {
            BoundedWorktree::Content(bytes) => Ok(bytes),
            BoundedWorktree::TooLarge(bytes) => Err(GitError::SourceTooLarge { bytes }),
        }
    }

    async fn read_worktree_bytes(&self, path: &RepoPath) -> Result<BoundedWorktree, GitError> {
        let joined = path::lexical_path(&self.root, path)?;
        let metadata = tokio::fs::symlink_metadata(&joined)
            .await
            .map_err(|source| GitError::Io {
                path: joined.clone(),
                source,
            })?;
        if metadata.file_type().is_symlink() {
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

    pub async fn ignored_paths(&self, relative: &[String]) -> Result<HashSet<String>, GitError> {
        if relative.is_empty() {
            return Ok(HashSet::new());
        }
        let mut stdin = Vec::new();
        for path in relative {
            stdin.extend_from_slice(path.as_bytes());
            stdin.push(0);
        }
        let output = command::run_with_stdin(
            &self.root,
            "check ignored paths",
            ["check-ignore", "-z", "--stdin"],
            &stdin,
            &[0, 1],
        )
        .await?;
        Ok(output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
            .map(|entry| String::from_utf8_lossy(entry).into_owned())
            .collect())
    }

    /// Stages selected paths. An empty slice is a no-op.
    ///
    /// # Errors
    ///
    /// Returns an error when a path is invalid or Git cannot stage it.
    async fn stage(&self, paths: &[RepoPath]) -> Result<(), GitError> {
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
    async fn unstage(&self, paths: &[RepoPath]) -> Result<(), GitError> {
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
    async fn stage_all(&self) -> Result<(), GitError> {
        command::run(&self.root, "stage all", ["add", "-A", "--"])
            .await
            .map(drop)
    }

    /// Unstages the complete index while preserving worktree contents.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot update the index.
    async fn unstage_all(&self) -> Result<(), GitError> {
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
    async fn commit(&self, message: &str) -> Result<(), GitError> {
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
    async fn discard(&self, path: &RepoPath, status: FileStatus) -> Result<(), GitError> {
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
            let output = command::run(&self.root, "resolve renamed path", STATUS_ARGS).await?;
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
        let output = command::run(
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
                .map_err(clankerdiff_core::DiffError::UnsupportedPathEncoding)?;
            let path = RepoPath::new(text)?;
            let captured = self.read_worktree_bytes(&path).await?;
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
        match command::run(
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
        // Diff has its own stat-cache refresh setting, independent of status.
        let mut args = vec![
            "-c",
            "diff.autoRefreshIndex=false",
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
