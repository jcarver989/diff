use crate::protocol::{SessionRequest, SessionResponseRef, read_request, write_response};
use diff_core::{DiffScope, RepositoryAction, ReviewSubmission};
use diff_git::{GitError, GitRepository, RepositorySnapshot};
use std::{
    io,
    os::unix::net::{UnixListener, UnixStream},
    path::Path,
};
use thiserror::Error;

pub fn run(
    repository: &GitRepository,
    snapshot: &RepositorySnapshot,
    scope: DiffScope,
    launch: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<Option<ReviewSubmission>, SessionError> {
    let directory = tempfile::Builder::new().prefix("clankerdiff-").tempdir()?;
    let socket_path = directory.path().join("session.sock");
    let listener = UnixListener::bind(&socket_path)?;
    launch(&socket_path).map_err(SessionError::Launch)?;

    let mut snapshot = snapshot.clone();
    for connection in listener.incoming() {
        let mut stream = connection?;
        match handle_connection(&mut stream, &mut snapshot, repository, scope) {
            Ok(ConnectionOutcome::Continue) => {}
            Ok(ConnectionOutcome::Submitted(submission)) => return Ok(Some(submission)),
            Ok(ConnectionOutcome::Cancelled) => return Ok(None),
            Err(error) => respond_to_bad_request(&mut stream, &error),
        }
    }
    Err(SessionError::ServerStopped)
}

fn respond_to_bad_request(stream: &mut UnixStream, error: &SessionError) {
    eprintln!("review session request failed: {error}");
    let message = error.to_string();
    let _ = write_response(stream, &SessionResponseRef::ProtocolError(&message));
}

fn handle_connection(
    stream: &mut UnixStream,
    snapshot: &mut RepositorySnapshot,
    repository: &GitRepository,
    scope: DiffScope,
) -> Result<ConnectionOutcome, SessionError> {
    match read_request(stream)? {
        SessionRequest::Document => {
            write_response(stream, &SessionResponseRef::Document(&snapshot.document))?;
            Ok(ConnectionOutcome::Continue)
        }
        SessionRequest::RepositoryAction(action) => {
            let result = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    execute_repository_action(repository, action).await?;
                    repository.snapshot_with_sources(scope).await
                })
            });
            match result {
                Ok(replacement) => {
                    *snapshot = replacement;
                    write_response(stream, &SessionResponseRef::Document(&snapshot.document))?;
                }
                Err(error) => {
                    write_response(
                        stream,
                        &SessionResponseRef::RepositoryError(&error.to_string()),
                    )?;
                }
            }
            Ok(ConnectionOutcome::Continue)
        }
        SessionRequest::Source(request) => {
            let response = snapshot.sources().response(&request);
            write_response(stream, &SessionResponseRef::Source(&response))?;
            Ok(ConnectionOutcome::Continue)
        }
        SessionRequest::Submit(submission) => {
            write_response(stream, &SessionResponseRef::Accepted)?;
            Ok(ConnectionOutcome::Submitted(submission))
        }
        SessionRequest::Cancel => {
            write_response(stream, &SessionResponseRef::Accepted)?;
            Ok(ConnectionOutcome::Cancelled)
        }
    }
}

async fn execute_repository_action(
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

enum ConnectionOutcome {
    Continue,
    Submitted(ReviewSubmission),
    Cancelled,
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("could not launch the review client: {0}")]
    Launch(String),
    #[error("the review session stopped before receiving feedback")]
    ServerStopped,
    #[error(transparent)]
    Protocol(#[from] crate::protocol::ProtocolError),
    #[error("review session I/O failed: {0}")]
    Io(#[from] io::Error),
}
