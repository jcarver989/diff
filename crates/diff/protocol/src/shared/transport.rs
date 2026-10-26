use async_channel::{Receiver, Sender};
use thiserror::Error;

pub struct LocalEnd<Out, In> {
    tx: Sender<Out>,
    rx: Receiver<In>,
}

#[derive(Debug, Error)]
#[error("local transport closed")]
pub struct TransportClosed;

impl<Out, In> LocalEnd<Out, In> {
    pub(crate) fn new(tx: Sender<Out>, rx: Receiver<In>) -> Self {
        Self { tx, rx }
    }

    pub async fn send(&self, message: Out) -> Result<(), TransportClosed> {
        self.tx.send(message).await.map_err(|_| TransportClosed)
    }

    pub async fn recv(&self) -> Result<In, TransportClosed> {
        self.rx.recv().await.map_err(|_| TransportClosed)
    }

    pub fn close(&self) {
        self.tx.close();
        self.rx.close();
    }
}
