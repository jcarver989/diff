use crate::protocol::{SessionRequest, SessionResponseRef, read_request, write_response};
use clankerdiff_core::{DiffScope, ReviewSubmission};
use clankerdiff_server::{DiffServer, ServerOptions};
use std::{io, path::Path, time::Duration};
use thiserror::Error;
use tokio::{net::UnixListener, time::timeout};

pub async fn run(
    path: &Path,
    scope: DiffScope,
    launch: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<Option<ReviewSubmission>, SessionError> {
    let server = DiffServer::open(path, ServerOptions::default()).await?;
    let listener = server.listen(([127, 0, 0, 1], 0).into()).await?;
    let url = format!("ws://{}/ws", listener.local_addr());
    let result = run_session(&url, scope, launch).await;
    server.shutdown().await?;
    listener.shutdown().await?;
    result
}

async fn run_session(
    url: &str,
    scope: DiffScope,
    launch: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<Option<ReviewSubmission>, SessionError> {
    let directory = tempfile::Builder::new().prefix("clankerdiff-").tempdir()?;
    let socket_path = directory.path().join("session.sock");
    let session_listener = UnixListener::bind(&socket_path)?;
    launch(&socket_path).map_err(SessionError::Launch)?;
    let (stream, _) = timeout(Duration::from_secs(60), session_listener.accept())
        .await
        .map_err(|_| SessionError::Timeout)??;
    let mut stream = stream.into_std()?;
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    if read_request(&mut stream)? != SessionRequest::Bootstrap {
        write_response(
            &mut stream,
            &SessionResponseRef::ProtocolError("expected bootstrap"),
        )?;
        return Err(SessionError::ServerStopped);
    }
    write_response(&mut stream, &SessionResponseRef::Bootstrap { url, scope })?;
    stream.set_read_timeout(None)?;
    let submission = match read_request(&mut stream)? {
        SessionRequest::Submit(submission) => Some(submission),
        SessionRequest::Cancel => None,
        SessionRequest::Bootstrap => return Err(SessionError::ServerStopped),
    };
    write_response(&mut stream, &SessionResponseRef::Accepted)?;
    Ok(submission)
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("could not launch the review client: {0}")]
    Launch(String),
    #[error("the review session stopped before receiving feedback")]
    ServerStopped,
    #[error("timed out waiting for the review client")]
    Timeout,
    #[error(transparent)]
    Server(#[from] clankerdiff_server::ServerError),
    #[error(transparent)]
    Protocol(#[from] crate::protocol::ProtocolError),
    #[error("review session I/O failed: {0}")]
    Io(#[from] io::Error),
}
