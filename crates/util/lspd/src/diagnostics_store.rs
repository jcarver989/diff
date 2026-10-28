use lsp_types::{PublishDiagnosticsParams, Uri};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::watch;
use tokio::time::{Instant, timeout_at};

#[derive(Clone)]
pub(crate) struct DiagnosticsStore {
    state: watch::Sender<DiagnosticsState>,
    settle: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Freshness {
    Fresh,
    Unconfirmed,
}

impl DiagnosticsStore {
    pub(crate) fn new(settle: Duration) -> Self {
        Self { state: watch::Sender::new(DiagnosticsState::default()), settle }
    }

    pub(crate) fn publish(&self, diagnostics: PublishDiagnosticsParams) {
        self.state.send_modify(|state| {
            state.generation += 1;
            state.diagnostics.insert(diagnostics.uri.clone(), (diagnostics, state.generation));
        });
    }

    pub(crate) fn current_uri_version(&self, uri: &Uri) -> u64 {
        self.state.borrow().uri_version(uri)
    }

    pub(crate) fn get(&self, uri: Option<&Uri>) -> Vec<PublishDiagnosticsParams> {
        let state = self.state.borrow();
        match uri {
            Some(uri) => state.diagnostics.get(uri).into_iter().map(|(diagnostics, _)| diagnostics.clone()).collect(),
            None => state.diagnostics.values().map(|(diagnostics, _)| diagnostics.clone()).collect(),
        }
    }

    pub(crate) fn uris(&self) -> Vec<Uri> {
        self.state.borrow().diagnostics.keys().cloned().collect()
    }

    pub(crate) fn forget_uri(&self, uri: &Uri) {
        self.state.send_if_modified(|state| {
            state.diagnostics.remove(uri);
            false
        });
    }

    pub(crate) async fn wait_for_uri_fresh(&self, uri: &Uri, version_before: u64, timeout: Duration) -> Freshness {
        let deadline = Instant::now() + timeout;
        let mut published = self.state.subscribe();
        let first = published.wait_for(|state| state.uri_version(uri) != version_before);
        if !matches!(timeout_at(deadline, first).await, Ok(Ok(_))) {
            return Freshness::Unconfirmed;
        }

        loop {
            let settled = self.current_uri_version(uri);
            let quiet_until = (Instant::now() + self.settle).min(deadline);
            let republished = published.wait_for(|state| state.uri_version(uri) != settled);
            if !matches!(timeout_at(quiet_until, republished).await, Ok(Ok(_))) {
                return Freshness::Fresh;
            }
        }
    }
}

#[derive(Default)]
struct DiagnosticsState {
    diagnostics: HashMap<Uri, (PublishDiagnosticsParams, u64)>,
    generation: u64,
}

impl DiagnosticsState {
    fn uri_version(&self, uri: &Uri) -> u64 {
        self.diagnostics.get(uri).map(|(_, version)| *version).unwrap_or_default()
    }
}
