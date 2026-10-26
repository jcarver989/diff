use super::FileWatcher;
use tokio::sync::mpsc::{self, error::TrySendError};

#[derive(Debug)]
pub struct FakeFileWatcher {
    events: mpsc::Receiver<()>,
}

impl FakeFileWatcher {
    #[must_use]
    pub fn new() -> (Self, FakeFileWatcherHandle) {
        let (sender, events) = mpsc::channel(1);
        (Self { events }, FakeFileWatcherHandle { sender })
    }
}

impl FileWatcher for FakeFileWatcher {
    async fn recv(&mut self) -> Option<()> {
        self.events.recv().await
    }
}

#[derive(Debug, Clone)]
pub struct FakeFileWatcherHandle {
    sender: mpsc::Sender<()>,
}

impl FakeFileWatcherHandle {
    pub fn trigger_notification(&self) -> Result<(), mpsc::error::SendError<()>> {
        match self.sender.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) => Ok(()),
            Err(TrySendError::Closed(())) => Err(mpsc::error::SendError(())),
        }
    }

    pub async fn closed(&self) {
        self.sender.closed().await;
    }
}
