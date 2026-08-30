use diff_core::{DiffDocument, DiffScope, RepositoryAction, ReviewSubmission};
use diff_git::{GitError, GitRepository};
use rand::RngCore;
use std::{
    fmt::Write as _,
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream},
};
use thiserror::Error;

const MAX_HEADERS: usize = 64 * 1024;
const MAX_SUBMISSION: usize = 8 * 1024 * 1024;

pub fn run(
    repository: &GitRepository,
    document: &DiffDocument,
    scope: DiffScope,
    launch: impl FnOnce(&str) -> Result<(), String>,
) -> Result<Option<ReviewSubmission>, SessionError> {
    let token = session_token();
    let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))?;
    let address = listener.local_addr()?;
    let url = format!("http://127.0.0.1:{}/session/{token}/", address.port());
    launch(&url).map_err(SessionError::Launch)?;

    let mut document_json = serde_json::to_vec(document)?;
    for connection in listener.incoming() {
        let mut stream = connection?;
        match handle_connection(&mut stream, &token, &mut document_json, repository, scope) {
            Ok(ConnectionOutcome::Continue) => {}
            Ok(ConnectionOutcome::Submitted(submission)) => return Ok(Some(submission)),
            Ok(ConnectionOutcome::Cancelled) => return Ok(None),
            Err(error) => {
                eprintln!("review session request failed: {error}");
                let _ = respond(
                    &mut stream,
                    400,
                    "text/plain; charset=utf-8",
                    b"bad request",
                );
            }
        }
    }
    Err(SessionError::ServerStopped)
}

fn handle_connection(
    stream: &mut TcpStream,
    token: &str,
    document: &mut Vec<u8>,
    repository: &GitRepository,
    scope: DiffScope,
) -> Result<ConnectionOutcome, SessionError> {
    let request = Request::read(stream)?;
    let document_path = format!("/api/{token}/document");
    let submit_path = format!("/api/{token}/submit");
    let cancel_path = format!("/api/{token}/cancel");
    let repository_action_path = format!("/api/{token}/repository-action");

    if request.method == "GET" && request.path == document_path {
        respond(stream, 200, "application/json", document)?;
        return Ok(ConnectionOutcome::Continue);
    }
    if request.method == "POST" && request.path == repository_action_path {
        let action: RepositoryAction = serde_json::from_slice(&request.body)?;
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                execute_repository_action(repository, action).await?;
                repository.snapshot(scope).await
            })
        });
        match result {
            Ok(snapshot) => {
                *document = serde_json::to_vec(&snapshot)?;
                respond(stream, 200, "application/json", document)?;
            }
            Err(error) => {
                respond(
                    stream,
                    422,
                    "text/plain; charset=utf-8",
                    error.to_string().as_bytes(),
                )?;
            }
        }
        return Ok(ConnectionOutcome::Continue);
    }
    if request.method == "POST" && request.path == submit_path {
        let submission = serde_json::from_slice(&request.body)?;
        respond(stream, 204, "text/plain", b"")?;
        return Ok(ConnectionOutcome::Submitted(submission));
    }
    if request.method == "POST" && request.path == cancel_path {
        respond(stream, 204, "text/plain", b"")?;
        return Ok(ConnectionOutcome::Cancelled);
    }

    respond(stream, 404, "text/plain; charset=utf-8", b"not found")?;
    Ok(ConnectionOutcome::Continue)
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

fn session_token() -> String {
    let mut bytes = [0_u8; 24];
    rand::rng().fill_bytes(&mut bytes);
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(token, "{byte:02x}");
    }
    token
}

fn respond(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
}

impl Request {
    fn read(stream: &mut TcpStream) -> Result<Self, SessionError> {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 8192];
        let header_end = loop {
            let count = stream.read(&mut chunk)?;
            if count == 0 {
                return Err(SessionError::MalformedRequest);
            }
            bytes.extend_from_slice(&chunk[..count]);
            if let Some(position) = find_bytes(&bytes, b"\r\n\r\n") {
                break position + 4;
            }
            if bytes.len() > MAX_HEADERS {
                return Err(SessionError::RequestTooLarge);
            }
        };

        let headers = std::str::from_utf8(&bytes[..header_end])?;
        let mut lines = headers.split("\r\n");
        let mut request_line = lines
            .next()
            .ok_or(SessionError::MalformedRequest)?
            .split_whitespace();
        let method = request_line
            .next()
            .ok_or(SessionError::MalformedRequest)?
            .to_owned();
        let path = request_line
            .next()
            .ok_or(SessionError::MalformedRequest)?
            .to_owned();
        let content_length = lines
            .filter_map(|line| line.split_once(':'))
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .map_or(Ok(0), |(_, value)| value.trim().parse::<usize>())
            .map_err(|_| SessionError::MalformedRequest)?;
        if content_length > MAX_SUBMISSION {
            return Err(SessionError::RequestTooLarge);
        }
        while bytes.len() < header_end + content_length {
            let count = stream.read(&mut chunk)?;
            if count == 0 {
                return Err(SessionError::MalformedRequest);
            }
            bytes.extend_from_slice(&chunk[..count]);
        }
        let body = bytes[header_end..header_end + content_length].to_vec();
        Ok(Self { method, path, body })
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
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
    #[error("malformed session request")]
    MalformedRequest,
    #[error("session request is too large")]
    RequestTooLarge,
    #[error("session request was not UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[error("invalid review submission: {0}")]
    Submission(#[from] serde_json::Error),
    #[error("review session I/O failed: {0}")]
    Io(#[from] io::Error),
}
