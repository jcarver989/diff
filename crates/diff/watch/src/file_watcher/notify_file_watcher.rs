use super::FileWatcher;
use ignore::WalkBuilder;
use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{Flag, ModifyKind},
};
use std::{
    collections::BTreeSet,
    future::Future,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    spawn,
    sync::{broadcast, broadcast::error::RecvError, mpsc},
    task::{JoinHandle, spawn_blocking},
    time::sleep,
};

const MAX_EVENTS: usize = 256;
const MAX_PATHS: usize = 1024;

#[derive(Debug)]
pub struct NotifyFileWatcher {
    rx: mpsc::Receiver<()>,
    task: JoinHandle<()>,
}

#[derive(Debug, thiserror::Error)]
pub enum FileWatchError {
    #[error("could not create filesystem watcher")]
    Create(#[source] notify::Error),
    #[error("could not watch {path}")]
    Watch {
        path: PathBuf,
        #[source]
        source: notify::Error,
    },
    #[error(transparent)]
    Walk(#[from] ignore::Error),
}

pub(crate) fn worktree_walk(root: impl AsRef<Path>) -> WalkBuilder {
    let mut walk = WalkBuilder::new(root);
    walk.hidden(false).ignore(false);
    walk
}

impl NotifyFileWatcher {
    pub fn new<T>(
        roots: impl IntoIterator<Item = PathBuf>,
        debounce: Duration,
        filter: impl FnMut(Vec<PathBuf>) -> T + Send + 'static,
    ) -> Result<Self, FileWatchError>
    where
        T: Future<Output = bool> + Send + 'static,
    {
        let walks = roots.into_iter().map(worktree_walk).collect();
        Self::with_walks(walks, debounce, filter)
    }

    pub(crate) fn with_walks<T>(
        walks: Vec<WalkBuilder>,
        debounce: Duration,
        filter: impl FnMut(Vec<PathBuf>) -> T + Send + 'static,
    ) -> Result<Self, FileWatchError>
    where
        T: Future<Output = bool> + Send + 'static,
    {
        let (events_tx, events_rx) = broadcast::channel(MAX_EVENTS);
        let watches = Watches::new(walks, events_tx)?;
        let (tx, rx) = mpsc::channel(1);
        Ok(Self {
            rx,
            task: spawn(process_events(tx, events_rx, watches, debounce, filter)),
        })
    }
}

impl FileWatcher for NotifyFileWatcher {
    async fn recv(&mut self) -> Option<()> {
        self.rx.recv().await
    }
}

impl Drop for NotifyFileWatcher {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct Watches {
    walks: Vec<WalkBuilder>,
    events_tx: broadcast::Sender<Event>,
    watcher: RecommendedWatcher,
}

impl Watches {
    fn new(
        walks: Vec<WalkBuilder>,
        events_tx: broadcast::Sender<Event>,
    ) -> Result<Self, FileWatchError> {
        let watcher = watch_all(&walks, &events_tx)?;
        Ok(Self {
            walks,
            events_tx,
            watcher,
        })
    }

    fn rebuild(mut self) -> Result<Self, FileWatchError> {
        self.watcher = watch_all(&self.walks, &self.events_tx)?;
        Ok(self)
    }
}

fn watch_all(
    walks: &[WalkBuilder],
    events_tx: &broadcast::Sender<Event>,
) -> Result<RecommendedWatcher, FileWatchError> {
    let events_tx = events_tx.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        let event = event.unwrap_or_else(|_| Event::new(EventKind::Any).set_flag(Flag::Rescan));
        let _ = events_tx.send(event);
    })
    .map_err(FileWatchError::Create)?;
    for entry in walks.iter().flat_map(WalkBuilder::build) {
        let entry = entry?;
        if entry.depth() == 0 || entry.file_type().is_some_and(|kind| kind.is_dir()) {
            let path = entry.into_path();
            watcher
                .watch(&path, RecursiveMode::NonRecursive)
                .map_err(|source| FileWatchError::Watch { path, source })?;
        }
    }
    Ok(watcher)
}

#[derive(Default)]
struct Batch {
    paths: BTreeSet<PathBuf>,
    rescan: bool,
    rewatch: bool,
}

impl Batch {
    fn push(&mut self, received: Result<Event, RecvError>) {
        let Ok(event) = received else {
            self.rescan = true;
            return;
        };
        if self.rescan || event.kind.is_access() {
            return;
        }
        if event.need_rescan() || event.paths.is_empty() {
            self.rescan = true;
            return;
        }
        self.rewatch |= needs_rewatch(&event);
        self.paths.extend(event.paths);
        self.rescan = self.paths.len() > MAX_PATHS;
    }
}

fn needs_rewatch(event: &Event) -> bool {
    !event.kind.is_modify()
        || matches!(event.kind, EventKind::Modify(ModifyKind::Name(_)))
        || event.paths.iter().any(|path| {
            path.file_name()
                .is_some_and(|name| name == ".gitignore" || name == "exclude" || name == "config")
        })
}

async fn process_events<T>(
    tx: mpsc::Sender<()>,
    mut events_rx: broadcast::Receiver<Event>,
    mut watches: Watches,
    debounce: Duration,
    mut filter: impl FnMut(Vec<PathBuf>) -> T,
) where
    T: Future<Output = bool>,
{
    loop {
        let mut batch = Batch::default();
        batch.push(events_rx.recv().await);
        let deadline = sleep(debounce);
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                biased;
                () = &mut deadline => break,
                received = events_rx.recv() => batch.push(received),
            }
        }
        if batch.rescan || batch.rewatch {
            watches = match spawn_blocking(move || watches.rebuild()).await {
                Ok(Ok(watches)) => watches,
                _ => return,
            };
        }
        if batch.rescan
            || (!batch.paths.is_empty() && filter(batch.paths.into_iter().collect()).await)
        {
            let _ = tx.try_send(());
        }
    }
}
