use crate::daemon::{DaemonError, DaemonResult, Timeouts};
use crate::protocol::{CallError, DaemonCall};
use crate::workspace_key::WorkspaceKey;
use crate::workspace_session::WorkspaceSession;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

pub(crate) type SessionRequest = (WorkspaceKey, oneshot::Sender<DaemonResult<WorkspaceSession>>);

#[derive(Clone)]
pub(crate) struct Workspace {
    key: WorkspaceKey,
    sessions: mpsc::Sender<SessionRequest>,
    timeouts: Timeouts,
}

impl Workspace {
    pub(crate) async fn open(
        key: WorkspaceKey,
        sessions: mpsc::Sender<SessionRequest>,
        timeouts: Timeouts,
    ) -> DaemonResult<Self> {
        let workspace = Self { key, sessions, timeouts };
        let session = workspace.current().await?;
        workspace
            .within_deadline(&session, session.wait_until_initialized())
            .await
            .unwrap_or(Err(DaemonError::LspInitializeTimedOut(timeouts.request)))?;
        Ok(workspace)
    }

    pub(crate) async fn call(&self, call: DaemonCall) -> Result<Value, CallError> {
        let session = self.current().await?;
        match call {
            DaemonCall::Lsp { method, params } => match self.lsp(&session, &method, &params).await {
                Err(CallError::Closed) => {
                    session.kill();
                    self.lsp(&self.current().await?, &method, &params).await
                }
                result => result,
            },
            DaemonCall::Diagnostics { uri } => {
                let diagnostics = self.within_deadline(&session, session.get_diagnostics(uri.as_ref())).await?;
                serde_json::to_value(diagnostics).map_err(|err| CallError::Daemon(err.to_string()))
            }
            DaemonCall::QueueRefresh { uri } => {
                session.queue_diagnostic_refresh(uri).await;
                Ok(Value::Null)
            }
        }
    }

    async fn lsp(&self, session: &WorkspaceSession, method: &str, params: &Value) -> Result<Value, CallError> {
        if params.get("position").is_some() {
            session.wait_until_ready(self.timeouts.request).await;
        }
        self.within_deadline(session, session.request(method, params)).await?
    }

    async fn within_deadline<T>(
        &self,
        session: &WorkspaceSession,
        work: impl Future<Output = T>,
    ) -> Result<T, CallError> {
        timeout(self.timeouts.request, work).await.map_err(|_| {
            session.kill();
            CallError::TimedOut { after: self.timeouts.request }
        })
    }

    async fn current(&self) -> DaemonResult<WorkspaceSession> {
        let (reply, current) = oneshot::channel();
        self.sessions.send((self.key.clone(), reply)).await.map_err(|_| DaemonError::ShuttingDown)?;
        current.await.map_err(|_| DaemonError::ShuttingDown)?
    }
}

impl From<DaemonError> for CallError {
    fn from(err: DaemonError) -> Self {
        Self::Daemon(err.to_string())
    }
}
