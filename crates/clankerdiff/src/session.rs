use crate::protocol::{SessionRequest, SessionResponseRef, read_request, write_response};
use clankerdiff_core::ReviewSubmission;
use clankerdiff_git::{GitRepository, RepositorySnapshot};
use clankerdiff_watch::RepositoryWatcher;
use std::{
    io,
    os::unix::net::{UnixListener, UnixStream},
    path::Path,
    sync::Arc,
};
use thiserror::Error;
use tokio::runtime::Handle;

pub fn run(
    repository: &GitRepository,
    watcher: &RepositoryWatcher,
    launch: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<Option<ReviewSubmission>, SessionError> {
    tokio::task::block_in_place(|| run_blocking(repository, watcher, launch))
}

fn run_blocking(
    repository: &GitRepository,
    watcher: &RepositoryWatcher,
    launch: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<Option<ReviewSubmission>, SessionError> {
    let directory = tempfile::Builder::new().prefix("clankerdiff-").tempdir()?;
    let socket_path = directory.path().join("session.sock");
    let listener = UnixListener::bind(&socket_path)?;
    let mut snapshot = watcher.snapshot_rx.borrow().as_ref().ok().cloned();
    let mut revision = u64::from(snapshot.is_some());
    launch(&socket_path).map_err(SessionError::Launch)?;

    for connection in listener.incoming() {
        let mut stream = connection?;
        match handle_connection(
            &mut stream,
            repository,
            watcher,
            &mut snapshot,
            &mut revision,
        ) {
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
    repository: &GitRepository,
    watcher: &RepositoryWatcher,
    snapshot: &mut Option<Arc<RepositorySnapshot>>,
    published_revision: &mut u64,
) -> Result<ConnectionOutcome, SessionError> {
    match read_request(stream)? {
        SessionRequest::Document { revision } => {
            let result = watcher.snapshot_rx.borrow().clone();
            let error = match result {
                Ok(latest) => {
                    if snapshot.as_ref() != Some(&latest) {
                        *snapshot = Some(latest);
                        *published_revision += 1;
                    }
                    None
                }
                Err(error) => Some(error.to_string()),
            };
            let background_error = error.as_deref();
            let response = match snapshot.as_ref() {
                Some(snapshot) if *published_revision != revision => SessionResponseRef::Document {
                    revision: *published_revision,
                    document: &snapshot.document,
                    background_error,
                },
                _ => SessionResponseRef::Unchanged { background_error },
            };
            write_response(stream, &response)?;
            Ok(ConnectionOutcome::Continue)
        }
        SessionRequest::RepositoryAction(action) => {
            let result = Handle::current().block_on(async {
                repository
                    .apply(action)
                    .await
                    .map_err(|error| error.to_string())
            });
            match result {
                Ok(()) => write_response(stream, &SessionResponseRef::Accepted)?,
                Err(error) => write_response(stream, &SessionResponseRef::RepositoryError(&error))?,
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{SessionRequestRef, SessionResponse, read_response, write_request};
    use clankerdiff_core::{DiffDocument, DiffScope, RepoPath, RepositoryAction, StageState};
    use clankerdiff_git::testing::{RepoFixture, RepoFixtureBuilder};
    use clankerdiff_watch::WatchOptions;
    use std::{
        fs,
        os::unix::net::UnixStream,
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    fn request(socket_path: &Path, request: &SessionRequestRef<'_>) -> SessionResponse {
        let mut stream = UnixStream::connect(socket_path).expect("connect to the review service");
        write_request(&mut stream, request).expect("write the request");
        read_response(&mut stream).expect("read the response")
    }

    fn exercise_protocol(socket_path: &Path, repo: &RepoFixture) -> Result<(), String> {
        let SessionResponse::Document {
            revision, document, ..
        } = request(socket_path, &SessionRequestRef::Document { revision: 0 })
        else {
            return Err("the initial request must return a document".to_owned());
        };
        if revision != 1 || document.files.is_empty() {
            return Err(format!(
                "unexpected initial document at revision {revision}"
            ));
        }

        let unchanged = request(socket_path, &SessionRequestRef::Document { revision });
        if unchanged
            != (SessionResponse::Unchanged {
                background_error: None,
            })
        {
            return Err(format!(
                "polling an unchanged revision returned {unchanged:?}"
            ));
        }

        let path = RepoPath::new("src/lib.rs").expect("valid fixture path");
        let action = RepositoryAction::StagePaths(vec![path]);
        if request(socket_path, &SessionRequestRef::RepositoryAction(&action))
            != SessionResponse::Accepted
        {
            return Err("staging must be accepted".to_owned());
        }

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match request(socket_path, &SessionRequestRef::Document { revision }) {
                SessionResponse::Document {
                    revision: staged,
                    document,
                    ..
                } if staged > revision => {
                    if document
                        .files
                        .iter()
                        .all(|file| file.staged == StageState::Staged)
                        && !document.files.is_empty()
                    {
                        return exercise_health(socket_path, repo, staged, &document);
                    }
                    return Err("the published document must contain staged changes".to_owned());
                }
                SessionResponse::Unchanged {
                    background_error: None,
                } if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(25));
                }
                other => {
                    return Err(format!(
                        "waiting for the watched mutation returned {other:?}"
                    ));
                }
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_polling_client_only_receives_changed_documents() {
        let repo = RepoFixtureBuilder::new()
            .file("src/lib.rs", "fn main() {}\n")
            .committed()
            .build();
        repo.write("src/lib.rs", "fn main() {}\nfn added() {}\n");
        let watcher = RepositoryWatcher::spawn(
            repo.repository().await,
            DiffScope::Both,
            WatchOptions::default(),
        )
        .await
        .expect("the watcher must start");

        let (report, outcome) = mpsc::channel();
        let submission = run(&repo.repository().await, &watcher, |socket_path| {
            let socket_path = socket_path.to_path_buf();
            thread::spawn(move || {
                // Cancel unconditionally so a failed assertion cannot leave
                // the single-connection server waiting forever.
                let _ = report.send(exercise_protocol(&socket_path, &repo));
                request(&socket_path, &SessionRequestRef::Cancel);
            });
            Ok(())
        })
        .expect("the session must run");

        outcome
            .recv()
            .expect("the client must report")
            .expect("the session protocol must hold");
        assert!(submission.is_none(), "cancelling returns no submission");
    }

    fn exercise_health(
        socket_path: &Path,
        repo: &RepoFixture,
        revision: u64,
        document: &DiffDocument,
    ) -> Result<(), String> {
        let index = fs::read(repo.root().join(".git/index")).map_err(|error| error.to_string())?;
        repo.write(".git/index", "corrupt index");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match request(socket_path, &SessionRequestRef::Document { revision }) {
                SessionResponse::Unchanged {
                    background_error: Some(_),
                } => break,
                SessionResponse::Unchanged {
                    background_error: None,
                } if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(25));
                }
                other => return Err(format!("waiting for a background error returned {other:?}")),
            }
        }
        match request(socket_path, &SessionRequestRef::Document { revision: 0 }) {
            SessionResponse::Document {
                revision: actual_revision,
                document: actual,
                background_error: Some(_),
            } if actual_revision == revision && &actual == document => {}
            other => {
                return Err(format!(
                    "the last good document was not retained: {other:?}"
                ));
            }
        }
        repo.write(".git/index", index);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match request(socket_path, &SessionRequestRef::Document { revision }) {
                SessionResponse::Unchanged {
                    background_error: None,
                } => return Ok(()),
                SessionResponse::Unchanged {
                    background_error: Some(_),
                } if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(25));
                }
                other => {
                    return Err(format!(
                        "equal-content recovery changed the document: {other:?}"
                    ));
                }
            }
        }
    }
}
