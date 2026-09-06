use super::FileWatcher;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::{future::Future, path::PathBuf, time::Duration};
use tokio::{spawn, sync::mpsc, task::JoinHandle, time::sleep};

#[derive(Debug)]
pub struct NotifyFileWatcher {
    _watcher: Option<RecommendedWatcher>,
    rx: mpsc::Receiver<()>,
    task: JoinHandle<()>,
}

#[derive(Debug, PartialEq, Eq)]
enum FilesChangedEvent {
    Paths(Vec<PathBuf>),
    Rescan,
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
}

impl NotifyFileWatcher {
    /// Starts a recursive file watcher with a caller-supplied filter.
    pub fn new<T>(
        roots: impl IntoIterator<Item = PathBuf>,
        debounce: Duration,
        filter: impl FnMut(Vec<PathBuf>) -> T + Send + 'static,
    ) -> Result<Self, FileWatchError>
    where
        T: Future<Output = bool> + Send + 'static,
    {
        let (files_changed_tx, files_changed_rx) = mpsc::unbounded_channel();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            if let Some(change) = match event {
                Ok(e) if e.need_rescan() => Some(FilesChangedEvent::Rescan),
                Ok(e) if e.kind.is_access() => None,
                Ok(e) if e.paths.is_empty() => Some(FilesChangedEvent::Rescan),
                Ok(e) => Some(FilesChangedEvent::Paths(e.paths)),
                Err(_) => Some(FilesChangedEvent::Rescan),
            } {
                let _ = files_changed_tx.send(change);
            }
        })
        .map_err(FileWatchError::Create)?;

        for path in watch_roots(roots) {
            watcher
                .watch(&path, RecursiveMode::Recursive)
                .map_err(|source| FileWatchError::Watch { path, source })?;
        }

        let (tx, rx) = mpsc::channel(1);
        Ok(Self {
            _watcher: Some(watcher),
            rx,
            task: spawn(process_events(tx, files_changed_rx, debounce, filter)),
        })
    }
}

impl FileWatcher for NotifyFileWatcher {
    async fn recv(&mut self) -> Option<()> {
        self.rx.recv().await
    }
}

async fn process_events<T>(
    tx: mpsc::Sender<()>,
    mut rx: mpsc::UnboundedReceiver<FilesChangedEvent>,
    debounce: Duration,
    mut filter: impl FnMut(Vec<PathBuf>) -> T,
) where
    T: Future<Output = bool>,
{
    while let Some(event) = rx.recv().await {
        let mut paths = Vec::new();
        let mut rescan = false;
        collect_event(event, &mut paths, &mut rescan);

        let deadline = sleep(debounce);
        tokio::pin!(deadline);
        let mut ended = false;
        loop {
            tokio::select! {
                biased;
                () = &mut deadline => break,
                change = rx.recv() => {
                    if let Some(change) = change { collect_event(change, &mut paths, &mut rescan); }
                    else { ended = true; break; }
                },
            }
        }
        paths.sort();
        paths.dedup();
        if rescan || (!paths.is_empty() && filter(paths).await) {
            let _ = tx.try_send(());
        }
        if ended {
            break;
        }
    }
}

fn collect_event(event: FilesChangedEvent, paths: &mut Vec<PathBuf>, rescan: &mut bool) {
    match event {
        FilesChangedEvent::Paths(batch) => paths.extend(batch),
        FilesChangedEvent::Rescan => *rescan = true,
    }
}

fn watch_roots(roots: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut roots: Vec<_> = roots.into_iter().collect();
    roots.sort();
    roots.dedup();
    roots
        .iter()
        .filter(|path| {
            !roots
                .iter()
                .any(|other| *path != other && path.starts_with(other))
        })
        .cloned()
        .collect()
}

impl Drop for NotifyFileWatcher {
    fn drop(&mut self) {
        self.task.abort();
    }
}
