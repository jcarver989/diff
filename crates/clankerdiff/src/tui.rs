use crate::{
    preferences::load_theme,
    protocol::{SessionRequestRef, SessionResponse, read_response, write_request},
};
use clankerdiff_client::{ClientState, DiffClient};
use clankerdiff_core::{DiffDocument, DiffReviewEvent, ReviewSubmission};
use clankerdiff_markdown::{MarkdownDocument, MarkdownReviewSubmission};
use clankerdiff_ratatui::{
    DiffReviewState, DiffReviewWidget, InputOutcome, MarkdownReviewEvent, MarkdownReviewState,
    MarkdownReviewWidget, ThemeChoice, handle_crossterm_event, handle_markdown_crossterm_event,
};
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    env,
    io::{self, IsTerminal, Write, stderr, stdin},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        mpsc::{self, Receiver, TryRecvError},
    },
    thread,
    time::Duration,
};
use thiserror::Error;
use tokio::runtime::Handle;

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

pub async fn attach(socket_path: PathBuf) -> Result<(), TuiError> {
    let mut stream = UnixStream::connect(socket_path)?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    write_request(&mut stream, &SessionRequestRef::Bootstrap)?;
    let response = read_response(&mut stream)?;
    let SessionResponse::Bootstrap { url, scope } = response else {
        return Err(TuiError::UnexpectedResponse(response));
    };
    let client = DiffClient::connect(&url, scope.into()).await?;
    let outcome = run_client(&client);
    client.close().await?;
    let submission = outcome?;
    let request = match &submission {
        Some(submission) => SessionRequestRef::Submit(submission),
        None => SessionRequestRef::Cancel,
    };
    write_request(&mut stream, &request)?;
    let response = read_response(&mut stream)?;
    if response != SessionResponse::Accepted {
        return Err(TuiError::UnexpectedResponse(response));
    }
    Ok(())
}

pub fn run_client(client: &DiffClient) -> Result<Option<ReviewSubmission>, TuiError> {
    tokio::task::block_in_place(|| run_diff_review(client))
}

fn run_diff_review(client: &DiffClient) -> Result<Option<ReviewSubmission>, TuiError> {
    if !stdin().is_terminal() || !stderr().is_terminal() {
        return Err(TuiError::NoTerminal);
    }
    let _session = TerminalSession::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stderr()))?;
    let mut terminal_size = terminal.size()?;
    let mut state = DiffReviewState::new(Arc::new(DiffDocument::empty()));
    state.set_theme(load_theme());
    state.set_theme_choices(ThemeChoice::catalog());
    let mut installed = None;
    let mut last_state: Option<Arc<ClientState>> = None;
    let mut operation: Option<Receiver<Result<(), String>>> = None;
    loop {
        let current = client.state();
        if last_state
            .as_ref()
            .is_none_or(|old| !Arc::ptr_eq(old, &current))
        {
            if let Some(snapshot) = current.snapshot_if_changed(&mut installed) {
                state.set_document(snapshot.document.clone());
                state.set_scope(snapshot.scope);
            }
            state.set_background_error(current.status());
            state.set_capabilities(current.capabilities);
            last_state = Some(current);
        }
        if let Some(reply) = &operation {
            match reply.try_recv() {
                Err(TryRecvError::Empty) => {}
                result => {
                    operation = None;
                    match result.unwrap_or_else(|_| Err("review command stopped".to_owned())) {
                        Ok(()) => state.clear_repository_pending(),
                        Err(error) => state.set_repository_error(error),
                    }
                }
            }
        }
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
        for _ in 0..MAX_COALESCED_EVENTS {
            let input = handle_crossterm_event(&mut state, event::read()?);
            if let InputOutcome::ThemeSelected(id) = &input {
                let _ = crate::preferences::save_theme(&id.to_string());
            }
            match input.into_event() {
                Some(DiffReviewEvent::Cancel) => return Ok(None),
                Some(DiffReviewEvent::SubmitReview(submission)) => return Ok(Some(submission)),
                Some(DiffReviewEvent::CopyFormattedReview(_)) | None => {}
                Some(command) if operation.is_none() => {
                    state.set_repository_pending();
                    let client = client.clone();
                    let (reply, receiver) = mpsc::channel();
                    operation = Some(receiver);
                    Handle::current().spawn(async move {
                        let result = client.handle(command).await;
                        let _ = reply.send(result.map_err(|error| error.to_string()));
                    });
                }
                Some(_) => {}
            }
            if !event::poll(Duration::ZERO)? {
                break;
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
    let mut terminal = Terminal::new(CrosstermBackend::new(stderr()))?;
    let mut terminal_size = terminal.size()?;
    let mut state = MarkdownReviewState::with_theme(document, crate::preferences::load_theme());
    state.set_theme_choices(ThemeChoice::catalog());
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
    let input = handle_markdown_crossterm_event(state, event)?;
    if let InputOutcome::ThemeSelected(id) = &input {
        let _ = crate::preferences::save_theme(&id.to_string());
    }
    Ok(match input.into_event() {
        Some(MarkdownReviewEvent::Submit(submission)) => {
            MarkdownEventOutcome::Submitted(submission)
        }
        Some(MarkdownReviewEvent::Cancel) => MarkdownEventOutcome::Cancelled,
        Some(MarkdownReviewEvent::CopyFormatted(_)) | None => MarkdownEventOutcome::Continue,
    })
}

enum MarkdownEventOutcome {
    Continue,
    Cancelled,
    Submitted(MarkdownReviewSubmission),
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
    #[error("the review service returned an unexpected response: {0:?}")]
    UnexpectedResponse(SessionResponse),
    #[error(transparent)]
    Client(#[from] clankerdiff_client::ClientError),
    #[error(transparent)]
    Transport(#[from] crate::protocol::ProtocolError),
    #[error("Markdown review action failed: {0}")]
    MarkdownReview(#[from] clankerdiff_markdown::MarkdownReviewError),
    #[error("terminal or review service I/O failed: {0}")]
    Io(#[from] io::Error),
}
