use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use diff_core::{DiffDocument, ReviewSubmission};
use diff_ratatui::{DiffReviewEvent, DiffReviewState, DiffReviewWidget, handle_crossterm_event};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    env,
    io::{self, IsTerminal, Read, Write, stderr, stdin},
    net::{Shutdown, SocketAddr, TcpStream},
    process::{Command, Stdio},
    sync::Arc,
    thread,
    time::Duration,
};
use thiserror::Error;

pub const COMMAND_ENV: &str = "CLANKERDIFF_TUI_COMMAND";

const MAX_COALESCED_EVENTS: usize = 256;
const MAX_RESPONSE_BYTES: usize = 128 * 1024 * 1024;
const ENABLE_MOUSE_BUTTONS: &[u8] = b"\x1b[?1000h\x1b[?1006h";
const DISABLE_MOUSE_BUTTONS: &[u8] = b"\x1b[?1006l\x1b[?1000l";

pub fn launch(session_url: &str) -> Result<(), TuiError> {
    let command = env::var(COMMAND_ENV).map_err(|_| TuiError::MissingCommand)?;
    let (program, arguments) = parse_command(&command)?;
    let executable = env::current_exe()?;
    let mut child = Command::new(program)
        .args(arguments)
        .arg(executable)
        .arg("attach")
        .arg(session_url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(TuiError::Launch)?;
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn parse_command(command: &str) -> Result<(String, Vec<String>), TuiError> {
    let mut words = shlex::split(command).ok_or(TuiError::InvalidCommand)?;
    if words.is_empty() {
        return Err(TuiError::InvalidCommand);
    }
    let program = words.remove(0);
    Ok((program, words))
}

pub fn attach(session_url: &str) -> Result<(), TuiError> {
    let session = Session::parse(session_url)?;
    let document = session.document()?;
    let outcome = run(Arc::new(document))?;
    session.complete(outcome.as_ref())?;
    Ok(())
}

fn run(document: Arc<DiffDocument>) -> Result<Option<ReviewSubmission>, TuiError> {
    if !stdin().is_terminal() || !stderr().is_terminal() {
        return Err(TuiError::NoTerminal);
    }

    let _session = TerminalSession::enter()?;
    let backend = CrosstermBackend::new(stderr());
    let mut terminal = Terminal::new(backend)?;
    let mut state = DiffReviewState::new(document);

    loop {
        if state.is_dirty() {
            terminal.draw(|frame| {
                frame.render_stateful_widget(DiffReviewWidget::new(), frame.area(), &mut state);
                if let Some(position) = state.cursor_position() {
                    frame.set_cursor_position(position);
                }
            })?;
        }

        match apply_event(&mut state, event::read()?) {
            EventOutcome::Continue => {}
            EventOutcome::Cancelled => return Ok(None),
            EventOutcome::Submitted(submission) => return Ok(Some(submission)),
        }
        for _ in 1..MAX_COALESCED_EVENTS {
            if !event::poll(Duration::ZERO)? {
                break;
            }
            match apply_event(&mut state, event::read()?) {
                EventOutcome::Continue => {}
                EventOutcome::Cancelled => return Ok(None),
                EventOutcome::Submitted(submission) => return Ok(Some(submission)),
            }
        }
    }
}

struct Session {
    address: SocketAddr,
    token: String,
}

impl Session {
    fn parse(url: &str) -> Result<Self, TuiError> {
        let value = url
            .strip_prefix("http://")
            .ok_or(TuiError::InvalidSessionUrl)?;
        let (authority, token) = value
            .split_once("/session/")
            .ok_or(TuiError::InvalidSessionUrl)?;
        let token = token.trim_end_matches('/');
        if token.is_empty() || token.contains('/') {
            return Err(TuiError::InvalidSessionUrl);
        }
        let address = authority.parse().map_err(|_| TuiError::InvalidSessionUrl)?;
        Ok(Self {
            address,
            token: token.to_owned(),
        })
    }

    fn document(&self) -> Result<DiffDocument, TuiError> {
        let path = format!("/api/{}/document", self.token);
        let body = self.request("GET", &path, &[])?;
        serde_json::from_slice(&body).map_err(TuiError::ProtocolJson)
    }

    fn complete(&self, submission: Option<&ReviewSubmission>) -> Result<(), TuiError> {
        let (path, body) = match submission {
            Some(submission) => (
                format!("/api/{}/submit", self.token),
                serde_json::to_vec(submission).map_err(TuiError::ProtocolJson)?,
            ),
            None => (format!("/api/{}/cancel", self.token), Vec::new()),
        };
        self.request("POST", &path, &body).map(|_| ())
    }

    fn request(&self, method: &str, path: &str, body: &[u8]) -> Result<Vec<u8>, TuiError> {
        let mut stream = TcpStream::connect(self.address)?;
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.address,
            body.len()
        )?;
        stream.write_all(body)?;
        stream.flush()?;
        stream.shutdown(Shutdown::Write)?;

        let mut response = Vec::new();
        stream
            .take((MAX_RESPONSE_BYTES + 1) as u64)
            .read_to_end(&mut response)?;
        if response.len() > MAX_RESPONSE_BYTES {
            return Err(TuiError::ResponseTooLarge);
        }
        let header_end = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .ok_or(TuiError::InvalidResponse)?
            + 4;
        let headers = std::str::from_utf8(&response[..header_end])?;
        let status = headers
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse::<u16>().ok())
            .ok_or(TuiError::InvalidResponse)?;
        if !(200..300).contains(&status) {
            return Err(TuiError::HttpStatus(status));
        }
        Ok(response[header_end..].to_vec())
    }
}

fn apply_event(state: &mut DiffReviewState, event: Event) -> EventOutcome {
    match handle_crossterm_event(state, event) {
        Some(DiffReviewEvent::Cancel) => EventOutcome::Cancelled,
        Some(DiffReviewEvent::SubmitReview(submission)) => EventOutcome::Submitted(submission),
        Some(DiffReviewEvent::CopyFormattedReview(_)) | None => EventOutcome::Continue,
    }
}

enum EventOutcome {
    Continue,
    Cancelled,
    Submitted(ReviewSubmission),
}

struct TerminalSession;

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(stderr(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        if let Err(error) = write_all_flushed(ENABLE_MOUSE_BUTTONS) {
            let _ = execute!(stderr(), LeaveAlternateScreen);
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = write_all_flushed(DISABLE_MOUSE_BUTTONS);
        let _ = disable_raw_mode();
        let _ = execute!(stderr(), LeaveAlternateScreen);
    }
}

fn write_all_flushed(bytes: &[u8]) -> io::Result<()> {
    let mut output = stderr();
    output.write_all(bytes)?;
    output.flush()
}

#[derive(Debug, Error)]
pub enum TuiError {
    #[error("{COMMAND_ENV} is required for TUI reviews")]
    MissingCommand,
    #[error("{COMMAND_ENV} must contain a valid, non-empty command")]
    InvalidCommand,
    #[error("could not launch the TUI command: {0}")]
    Launch(#[source] io::Error),
    #[error("the TUI requires interactive stdin and stderr")]
    NoTerminal,
    #[error("invalid review session URL")]
    InvalidSessionUrl,
    #[error("the review service returned malformed HTTP")]
    InvalidResponse,
    #[error("the review service returned HTTP {0}")]
    HttpStatus(u16),
    #[error("the review service response was too large")]
    ResponseTooLarge,
    #[error("the review service returned invalid UTF-8: {0}")]
    ProtocolUtf8(#[from] std::str::Utf8Error),
    #[error("the review service returned invalid JSON: {0}")]
    ProtocolJson(#[source] serde_json::Error),
    #[error("terminal or review service I/O failed: {0}")]
    Io(#[from] io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_launcher_command() {
        let (program, arguments) =
            parse_command("ghostty +new-window --title='Clanker Diff Review' -e").unwrap();
        assert_eq!(program, "ghostty");
        assert_eq!(
            arguments,
            ["+new-window", "--title=Clanker Diff Review", "-e"]
        );
        assert!(parse_command("").is_err());
        assert!(parse_command("'").is_err());
    }

    #[test]
    fn parses_session_url() {
        let session = Session::parse("http://127.0.0.1:4321/session/secret/").unwrap();
        assert_eq!(session.address, "127.0.0.1:4321".parse().unwrap());
        assert_eq!(session.token, "secret");
        assert!(Session::parse("https://127.0.0.1:4321/session/secret/").is_err());
        assert!(Session::parse("http://127.0.0.1:4321/session/").is_err());
    }
}
