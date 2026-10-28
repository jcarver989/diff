use crate::uri::path_to_uri;
use globset::{Glob, GlobSet, GlobSetBuilder};
use lsp_types::{FileChangeType, FileEvent, FileSystemWatcher, GlobPattern, Uri, WatchKind};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::OsStr;
use std::future::pending;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{Sleep, sleep};

const DEBOUNCE: Duration = Duration::from_millis(200);

pub(crate) struct WatchedFiles {
    _watcher: Option<RecommendedWatcher>,
    root: PathBuf,
    pub(crate) events: mpsc::UnboundedReceiver<Event>,
    pub(crate) debounce: Option<Pin<Box<Sleep>>>,
    registrations: HashMap<String, Vec<FileSystemWatcher>>,
    globs: GlobSet,
    kinds: Vec<WatchKind>,
    changes: BTreeMap<Uri, FileChangeType>,
    discovered: BTreeSet<Uri>,
}

pub(crate) struct WatchBatch {
    pub(crate) changes: Vec<FileEvent>,
    pub(crate) discovered: Vec<Uri>,
}

impl WatchedFiles {
    pub(crate) fn new(root: &Path) -> Self {
        let (tx, events) = mpsc::unbounded_channel();
        let watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            if let Ok(event) = event {
                let _ = tx.send(event);
            }
        })
        .and_then(|mut watcher| watcher.watch(root, RecursiveMode::Recursive).map(|()| watcher))
        .inspect_err(|err| tracing::error!("Failed to start file watcher: {err}"))
        .ok();
        Self {
            _watcher: watcher,
            root: root.to_path_buf(),
            events,
            debounce: None,
            registrations: HashMap::new(),
            globs: GlobSet::empty(),
            kinds: Vec::new(),
            changes: BTreeMap::new(),
            discovered: BTreeSet::new(),
        }
    }

    pub(crate) fn register(&mut self, id: String, watchers: Vec<FileSystemWatcher>) {
        self.registrations.insert(id, watchers);
        self.rebuild();
    }

    pub(crate) fn unregister(&mut self, id: &str) {
        if self.registrations.remove(id).is_some() {
            self.rebuild();
        }
    }

    pub(crate) fn accumulate(&mut self, event: &Event) {
        for path in &event.paths {
            let relative = path.strip_prefix(&self.root).ok();
            let matches = self.globs.matches(relative.unwrap_or(path));
            let matches = if matches.is_empty() { self.globs.matches(path) } else { matches };
            if matches.is_empty() {
                if relative.is_some_and(|relative| tracks_implicitly(event.kind, relative, path))
                    && change_type(event.kind, WatchKind::all()).is_some()
                    && let Ok(uri) = path_to_uri(path)
                {
                    self.discovered.insert(uri);
                }
                continue;
            }
            let kinds = matches.iter().fold(WatchKind::empty(), |kinds, &index| kinds | self.kinds[index]);
            if let Some(change) = change_type(event.kind, kinds)
                && let Ok(uri) = path_to_uri(path)
            {
                self.changes.insert(uri, change);
            }
        }
        if !self.changes.is_empty() || !self.discovered.is_empty() {
            self.debounce = Some(Box::pin(sleep(DEBOUNCE)));
        }
    }

    pub(crate) fn take(&mut self) -> WatchBatch {
        self.debounce = None;
        let changes = std::mem::take(&mut self.changes).into_iter().map(|(uri, typ)| FileEvent { uri, typ }).collect();
        WatchBatch { changes, discovered: std::mem::take(&mut self.discovered).into_iter().collect() }
    }

    fn rebuild(&mut self) {
        let mut builder = GlobSetBuilder::new();
        self.kinds.clear();
        for watcher in self.registrations.values().flatten() {
            let pattern = match &watcher.glob_pattern {
                GlobPattern::String(pattern) => pattern,
                GlobPattern::Relative(relative) => &relative.pattern,
            };
            match Glob::new(pattern) {
                Ok(glob) => {
                    builder.add(glob);
                    self.kinds.push(watcher.kind.unwrap_or(WatchKind::all()));
                }
                Err(err) => tracing::warn!("Invalid glob pattern '{pattern}': {err}"),
            }
        }
        self.globs = builder.build().unwrap_or_else(|err| {
            tracing::error!("Failed to build glob set: {err}");
            GlobSet::empty()
        });
    }
}

pub(crate) async fn elapsed(debounce: &mut Option<Pin<Box<Sleep>>>) {
    match debounce {
        Some(timer) => timer.as_mut().await,
        None => pending().await,
    }
}

pub(crate) fn is_ignored_directory_name(name: &OsStr) -> bool {
    matches!(name.to_string_lossy().as_ref(), ".git" | "node_modules" | ".next" | "dist" | "build" | "target")
}

fn change_type(kind: EventKind, watched: WatchKind) -> Option<FileChangeType> {
    match kind {
        EventKind::Create(_) if watched.contains(WatchKind::Create) => Some(FileChangeType::CREATED),
        EventKind::Modify(_) if watched.contains(WatchKind::Change) => Some(FileChangeType::CHANGED),
        EventKind::Remove(_) if watched.contains(WatchKind::Delete) => Some(FileChangeType::DELETED),
        _ => None,
    }
}

fn tracks_implicitly(kind: EventKind, relative: &Path, absolute: &Path) -> bool {
    let ignored = relative
        .components()
        .any(|component| matches!(component, Component::Normal(name) if is_ignored_directory_name(name)));
    !ignored && (matches!(kind, EventKind::Remove(_)) || !absolute.is_dir())
}
