use crate::protocol::{SessionRequestRef, SessionResponse, read_response, write_request};
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use diff_core::{
    DiffDocument, MarkdownDocument, MarkdownReviewSubmission, RepositoryAction, ReviewSubmission,
};
use diff_ratatui::{
    DiffReviewEvent, DiffReviewState, DiffReviewWidget, MarkdownReviewEvent, MarkdownReviewState,
    MarkdownReviewWidget, handle_crossterm_event, handle_markdown_crossterm_event,
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    env,
    io::{self, IsTerminal, Write, stderr, stdin},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    thread,
    time::Duration,
};
use thiserror::Error;

pub const COMMAND_ENV: &str = "CLANKERDIFF_TUI_COMMAND";

const COMMAND_PLACEHOLDER: &str = "{command}";
const MAX_COALESCED_EVENTS: usize = 256;
const ENABLE_MOUSE_BUTTONS: &[u8] = b"\x1b[?1000h\x1b[?1006h";
const DISABLE_MOUSE_BUTTONS: &[u8] = b"\x1b[?1006l\x1b[?1000l";

pub fn launch(socket_path: &Path) -> Result<(), TuiError> {
    let launcher = env::var(COMMAND_ENV).map_err(|_| TuiError::MissingCommand)?;
    let (program, mut arguments) = parse_command(&launcher)?;
    let executable = env::current_exe()?;
    let attach_command = attach_command(&executable, socket_path)?;
    let uses_placeholder = replace_command_placeholder(&mut arguments, &attach_command);

    let mut command = Command::new(program);
    command.args(arguments);
    if !uses_placeholder {
        command.arg(executable).arg("attach").arg(socket_path);
    }
    let mut child = command
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

fn attach_command(executable: &Path, socket_path: &Path) -> Result<String, TuiError> {
    let executable = executable.to_str().ok_or(TuiError::NonUtf8CommandPath)?;
    let socket_path = socket_path.to_str().ok_or(TuiError::NonUtf8CommandPath)?;
    shlex::try_join([executable, "attach", socket_path]).map_err(|_| TuiError::InvalidCommand)
}

fn replace_command_placeholder(arguments: &mut [String], command: &str) -> bool {
    let mut replaced = false;
    for argument in arguments {
        if argument.contains(COMMAND_PLACEHOLDER) {
            *argument = argument.replace(COMMAND_PLACEHOLDER, command);
            replaced = true;
        }
    }
    replaced
}

pub fn attach(socket_path: PathBuf) -> Result<(), TuiError> {
    let session = Session::new(socket_path);
    let document = session.document()?;
    let outcome = run(Arc::new(document), &session)?;
    session.complete(outcome.as_ref())?;
    Ok(())
}

fn run(
    document: Arc<DiffDocument>,
    session: &Session,
) -> Result<Option<ReviewSubmission>, TuiError> {
    if !stdin().is_terminal() || !stderr().is_terminal() {
        return Err(TuiError::NoTerminal);
    }

    let _session = TerminalSession::enter()?;
    let backend = CrosstermBackend::new(stderr());
    let mut terminal = Terminal::new(backend)?;
    let mut terminal_size = terminal.size()?;
    let mut state = DiffReviewState::with_theme(document, crate::preferences::load_theme());

    loop {
        terminal.autoresize()?;
        let current_size = terminal.size()?;
        if current_size != terminal_size {
            terminal_size = current_size;
            state.mark_dirty();
        }
        if state.is_dirty() {
            terminal.draw(|frame| {
                frame.render_stateful_widget(DiffReviewWidget::new(), frame.area(), &mut state);
                if let Some(position) = state.cursor_position() {
                    frame.set_cursor_position(position);
                }
            })?;
        }

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        match apply_event(&mut state, event::read()?, session) {
            EventOutcome::Continue => {}
            EventOutcome::Cancelled => return Ok(None),
            EventOutcome::Submitted(submission) => return Ok(Some(submission)),
        }
        for _ in 1..MAX_COALESCED_EVENTS {
            if !event::poll(Duration::ZERO)? {
                break;
            }
            match apply_event(&mut state, event::read()?, session) {
                EventOutcome::Continue => {}
                EventOutcome::Cancelled => return Ok(None),
                EventOutcome::Submitted(submission) => return Ok(Some(submission)),
            }
        }
    }
}

pub fn run_markdown(
    document: Arc<MarkdownDocument>,
) -> Result<Option<MarkdownReviewSubmission>, TuiError> {
    if !stdin().is_terminal() || !stderr().is_terminal() {
        return Err(TuiError::NoTerminal);
    }

    let _session = TerminalSession::enter()?;
    let backend = CrosstermBackend::new(stderr());
    let mut terminal = Terminal::new(backend)?;
    let mut terminal_size = terminal.size()?;
    let mut state = MarkdownReviewState::with_theme(document, crate::preferences::load_theme());
    loop {
        terminal.autoresize()?;
        let current_size = terminal.size()?;
        if current_size != terminal_size {
            terminal_size = current_size;
            state.mark_dirty();
        }
        if state.is_dirty() {
            terminal.draw(|frame| {
                frame.render_stateful_widget(MarkdownReviewWidget::new(), frame.area(), &mut state);
                if let Some(position) = state.cursor_position() {
                    frame.set_cursor_position(position);
                }
            })?;
        }
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        match apply_markdown_event(&mut state, event::read()?)? {
            MarkdownEventOutcome::Continue => {}
            MarkdownEventOutcome::Cancelled => return Ok(None),
            MarkdownEventOutcome::Submitted(submission) => return Ok(Some(submission)),
        }
    }
}

fn apply_markdown_event(
    state: &mut MarkdownReviewState,
    event: Event,
) -> Result<MarkdownEventOutcome, TuiError> {
    let previous_theme = state.theme().id().to_string();
    let outcome = match handle_markdown_crossterm_event(state, event)? {
        Some(MarkdownReviewEvent::Submit(submission)) => {
            MarkdownEventOutcome::Submitted(submission)
        }
        Some(MarkdownReviewEvent::Cancel) => MarkdownEventOutcome::Cancelled,
        Some(MarkdownReviewEvent::CopyFormatted(_)) | None => MarkdownEventOutcome::Continue,
    };
    let current_theme = state.theme().id().to_string();
    if current_theme != previous_theme {
        let _ = crate::preferences::save_theme(&current_theme);
    }
    Ok(outcome)
}

enum MarkdownEventOutcome {
    Continue,
    Cancelled,
    Submitted(MarkdownReviewSubmission),
}

struct Session {
    socket_path: PathBuf,
}

impl Session {
    fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    fn document(&self) -> Result<DiffDocument, TuiError> {
        match self.request(&SessionRequestRef::Document)? {
            SessionResponse::Document(document) => Ok(document),
            response => Err(TuiError::UnexpectedResponse(response)),
        }
    }

    fn repository_action(&self, action: &RepositoryAction) -> Result<DiffDocument, TuiError> {
        match self.request(&SessionRequestRef::RepositoryAction(action))? {
            SessionResponse::Document(document) => Ok(document),
            SessionResponse::RepositoryError(message) => Err(TuiError::Repository(message)),
            response => Err(TuiError::UnexpectedResponse(response)),
        }
    }

    fn complete(&self, submission: Option<&ReviewSubmission>) -> Result<(), TuiError> {
        let request = match submission {
            Some(submission) => SessionRequestRef::Submit(submission),
            None => SessionRequestRef::Cancel,
        };
        match self.request(&request)? {
            SessionResponse::Accepted => Ok(()),
            response => Err(TuiError::UnexpectedResponse(response)),
        }
    }

    fn request(&self, request: &SessionRequestRef<'_>) -> Result<SessionResponse, TuiError> {
        let mut stream = UnixStream::connect(&self.socket_path)?;
        write_request(&mut stream, request)?;
        let response = read_response(&mut stream)?;
        match response {
            SessionResponse::ProtocolError(message) => Err(TuiError::Protocol(message)),
            response => Ok(response),
        }
    }
}

fn apply_event(state: &mut DiffReviewState, event: Event, session: &Session) -> EventOutcome {
    let previous_theme = state.theme().id().to_string();
    let outcome = match handle_crossterm_event(state, event) {
        Some(DiffReviewEvent::RepositoryAction(action)) => {
            state.set_repository_pending();
            match session.repository_action(&action) {
                Ok(document) => state.set_document(Arc::new(document)),
                Err(error) => state.set_repository_error(error.to_string()),
            }
            EventOutcome::Continue
        }
        Some(DiffReviewEvent::Cancel) => EventOutcome::Cancelled,
        Some(DiffReviewEvent::SubmitReview(submission)) => EventOutcome::Submitted(submission),
        Some(DiffReviewEvent::CopyFormattedReview(_)) | None => EventOutcome::Continue,
    };
    let current_theme = state.theme().id().to_string();
    if current_theme != previous_theme {
        let _ = crate::preferences::save_theme(&current_theme);
    }
    outcome
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
    #[error("the TUI executable and socket paths must be valid UTF-8")]
    NonUtf8CommandPath,
    #[error("could not launch the TUI command: {0}")]
    Launch(#[source] io::Error),
    #[error("the TUI requires interactive stdin and stderr")]
    NoTerminal,
    #[error("the review service rejected the request: {0}")]
    Protocol(String),
    #[error("the review service returned an unexpected response: {0:?}")]
    UnexpectedResponse(SessionResponse),
    #[error("repository action failed: {0}")]
    Repository(String),
    #[error(transparent)]
    Transport(#[from] crate::protocol::ProtocolError),
    #[error("Markdown review action failed: {0}")]
    MarkdownReview(#[from] diff_core::MarkdownReviewError),
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
    fn expands_a_shell_escaped_command_placeholder() {
        let command = attach_command(
            Path::new("/Applications/Clanker Diff"),
            Path::new("/tmp/review session.sock"),
        )
        .unwrap();
        let mut arguments = vec!["--initial-command={command}".to_owned()];
        assert!(replace_command_placeholder(&mut arguments, &command));
        assert_eq!(
            arguments,
            ["--initial-command='/Applications/Clanker Diff' attach '/tmp/review session.sock'"]
        );
    }
}
