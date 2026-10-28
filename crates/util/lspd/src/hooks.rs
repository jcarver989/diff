use lsp_types::Uri;
use std::future::{Future, poll_fn};
use std::pin::pin;
use tokio::sync::watch;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestPoint {
    ReadyWait,
    RequestReceived(String),
    DocumentOpen(Uri),
    DocumentSyncWait(Uri),
    DiagnosticsWait(Uri),
}

#[derive(Clone, Default)]
pub struct TestGate {
    pub(crate) state: watch::Sender<(bool, bool)>,
}

#[derive(Clone, Default)]
pub(crate) struct TestHooks(pub(crate) Vec<(TestPoint, TestGate)>);

impl TestHooks {
    pub(crate) async fn pause(&self, point: TestPoint) {
        if let Some(gate) = self.gate(&point) {
            gate.state.send_modify(|(reached, _)| *reached = true);
            let _ = gate.state.subscribe().wait_for(|(_, released)| *released).await;
        }
    }

    pub(crate) async fn observe_pending<T>(&self, point: TestPoint, future: impl Future<Output = T>) -> T {
        let mut future = pin!(future);
        poll_fn(|cx| {
            let result = future.as_mut().poll(cx);
            if result.is_pending()
                && let Some(gate) = self.gate(&point)
            {
                gate.state.send_modify(|(reached, _)| *reached = true);
            }
            result
        })
        .await
    }

    fn gate(&self, point: &TestPoint) -> Option<&TestGate> {
        self.0.iter().find_map(|(at, gate)| (at == point).then_some(gate))
    }
}
