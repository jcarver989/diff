//! Serialized repository work for the native desktop host.

use async_channel::{Receiver, Sender, TryRecvError};
use diff_core::{DiffScope, RepositoryAction};
use diff_git::{
    GitError, GitRepository, RepositorySnapshot, RepositoryWatchEvent, RepositoryWatcher,
};
use std::path::PathBuf;

/// Completed repository state delivered to the GPUI entity.
pub(crate) enum RepositoryUpdate {
    MutationPending,
    Snapshot(RepositorySnapshot),
    Error { message: String, initial: bool },
    WatcherWarning(String),
}

/// Nonblocking command handle used by the GPUI entity.
#[derive(Clone)]
pub(crate) struct RepositoryClient {
    commands: Sender<RepositoryCommand>,
    invalidations: Sender<()>,
}

impl RepositoryClient {
    pub(crate) fn refresh(&self) {
        let _ = self.invalidations.try_send(());
    }

    pub(crate) fn set_scope(&self, scope: DiffScope) {
        let _ = self.commands.try_send(RepositoryCommand::SetScope(scope));
    }

    pub(crate) fn mutate(&self, action: RepositoryAction) {
        if action == RepositoryAction::Refresh {
            self.refresh();
        } else {
            let _ = self.commands.try_send(RepositoryCommand::Mutate(action));
        }
    }
}

pub(crate) struct RepositoryWorker {
    repository_path: PathBuf,
    repository: Option<GitRepository>,
    scope: DiffScope,
    watcher: Option<RepositoryWatcher>,
    watch_events: Option<Receiver<RepositoryWatchEvent>>,
    watching_disabled: bool,
    commands: Receiver<RepositoryCommand>,
    invalidations: Receiver<()>,
    updates: Sender<RepositoryUpdate>,
    has_snapshot: bool,
}

impl RepositoryWorker {
    pub(crate) fn new(
        repository_path: PathBuf,
        scope: DiffScope,
    ) -> (
        RepositoryClient,
        Receiver<RepositoryUpdate>,
        RepositoryWorker,
    ) {
        let (command_sender, commands) = async_channel::unbounded();
        let (invalidation_sender, invalidations) = async_channel::bounded(1);
        let (updates, update_receiver) = async_channel::unbounded();
        let client = RepositoryClient {
            commands: command_sender,
            invalidations: invalidation_sender,
        };
        let worker = Self {
            repository_path,
            repository: None,
            scope,
            watcher: None,
            watch_events: None,
            watching_disabled: false,
            commands,
            invalidations,
            updates,
            has_snapshot: false,
        };
        (client, update_receiver, worker)
    }

    pub(crate) async fn run(mut self) {
        self.reload().await;
        loop {
            match self.next_work().await {
                Work::Command(RepositoryCommand::SetScope(scope)) => {
                    self.scope = scope;
                    self.reload().await;
                }
                Work::Command(RepositoryCommand::Mutate(action)) => {
                    self.mutate(action).await;
                }
                Work::Refresh => {
                    if let Some(error) = self.coalesce_refreshes() {
                        self.disable_watcher(&error);
                    }
                    self.reload().await;
                }
                Work::WatcherError(error) => self.disable_watcher(&error),
                Work::Closed => break,
            }
        }
    }

    async fn next_work(&self) -> Work {
        if let Some(watch_events) = &self.watch_events {
            tokio::select! {
                biased;
                command = self.commands.recv() => command.map_or(Work::Closed, Work::Command),
                invalidation = self.invalidations.recv() => invalidation.map_or(Work::Closed, |()| Work::Refresh),
                event = watch_events.recv() => match event {
                    Ok(RepositoryWatchEvent::Changed) => Work::Refresh,
                    Ok(RepositoryWatchEvent::Error(error)) => Work::WatcherError(error),
                    Err(_) => Work::WatcherError("repository watcher stopped unexpectedly".to_owned()),
                },
            }
        } else {
            tokio::select! {
                biased;
                command = self.commands.recv() => command.map_or(Work::Closed, Work::Command),
                invalidation = self.invalidations.recv() => invalidation.map_or(Work::Closed, |()| Work::Refresh),
            }
        }
    }

    fn coalesce_refreshes(&self) -> Option<String> {
        while self.invalidations.try_recv().is_ok() {}
        let Some(watch_events) = &self.watch_events else {
            return None;
        };
        loop {
            match watch_events.try_recv() {
                Ok(RepositoryWatchEvent::Changed) => {}
                Ok(RepositoryWatchEvent::Error(error)) => return Some(error),
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Closed) => {
                    return Some("repository watcher stopped unexpectedly".to_owned());
                }
            }
        }
    }

    async fn reload(&mut self) {
        let discovered = if self.repository.is_none() {
            match GitRepository::discover(&self.repository_path).await {
                Ok(repository) => {
                    self.repository = Some(repository);
                    true
                }
                Err(error) => {
                    self.send_error(&error);
                    return;
                }
            }
        } else {
            false
        };
        let repository = self
            .repository
            .as_ref()
            .expect("repository was discovered before loading a snapshot");
        match repository.snapshot_with_sources(self.scope).await {
            Ok(snapshot) => {
                self.has_snapshot = true;
                let _ = self.updates.try_send(RepositoryUpdate::Snapshot(snapshot));
                if discovered || (self.watcher.is_none() && !self.watching_disabled) {
                    self.start_watching();
                }
            }
            Err(error) => self.send_error(&error),
        }
    }

    async fn mutate(&mut self, action: RepositoryAction) {
        let Some(repository) = self.repository.as_ref() else {
            return;
        };
        let _ = self.updates.try_send(RepositoryUpdate::MutationPending);
        let result = apply_action(repository, action).await;
        if let Err(error) = result {
            self.send_error(&error);
            return;
        }
        self.reload().await;
    }

    fn start_watching(&mut self) {
        let Some(repository) = &self.repository else {
            return;
        };
        match RepositoryWatcher::new(repository.root()) {
            Ok(watcher) => {
                self.watch_events = Some(watcher.receiver());
                self.watcher = Some(watcher);
            }
            Err(error) => {
                self.watching_disabled = true;
                let _ = self
                    .updates
                    .try_send(RepositoryUpdate::WatcherWarning(format!(
                        "repository auto-refresh unavailable: {error}"
                    )));
            }
        }
    }

    fn disable_watcher(&mut self, error: &str) {
        self.watcher = None;
        self.watch_events = None;
        self.watching_disabled = true;
        let _ = self
            .updates
            .try_send(RepositoryUpdate::WatcherWarning(format!(
                "repository auto-refresh stopped: {error}"
            )));
    }

    fn send_error(&self, error: &GitError) {
        let _ = self.updates.try_send(RepositoryUpdate::Error {
            message: error.to_string(),
            initial: !self.has_snapshot,
        });
    }
}

#[derive(Debug)]
enum RepositoryCommand {
    SetScope(DiffScope),
    Mutate(RepositoryAction),
}

enum Work {
    Command(RepositoryCommand),
    Refresh,
    WatcherError(String),
    Closed,
}

async fn apply_action(
    repository: &GitRepository,
    action: RepositoryAction,
) -> Result<(), GitError> {
    match action {
        RepositoryAction::StagePaths(paths) => repository.stage(&paths).await,
        RepositoryAction::UnstagePaths(paths) => repository.unstage(&paths).await,
        RepositoryAction::StageAll => repository.stage_all().await,
        RepositoryAction::UnstageAll => repository.unstage_all().await,
        RepositoryAction::Commit { message } => repository.commit(&message).await,
        RepositoryAction::Discard { path, status } => repository.discard(&path, status).await,
        RepositoryAction::Refresh => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_requests_coalesce_without_dropping_commands() {
        let (client, _updates, worker) = RepositoryWorker::new(".".into(), DiffScope::Both);

        client.refresh();
        client.refresh();
        client.refresh();
        client.set_scope(DiffScope::Staged);
        client.mutate(RepositoryAction::StageAll);

        assert_eq!(worker.invalidations.len(), 1);
        assert_eq!(worker.commands.len(), 2);
    }
}
