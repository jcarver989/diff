use crate::protocol::{SessionRequestRef, SessionResponse, read_response, write_request};
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use diff_core::{
    DiffDocument, DiffScope, RepositoryAction, ReviewSubmission, SourceRequest, SourceResponse,
    SourceUnavailable,
};
use diff_git::{GitRepository, RepositorySnapshot, SourceArchive};
use diff_markdown::{MarkdownDocument, MarkdownReviewSubmission};
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
    sync::{Arc, mpsc},
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
    let mut backend = SessionBackend::new(socket_path);
    let document = backend.document()?;
    let outcome = run_diff_review(Arc::new(document), &mut backend)?;
    backend.complete(outcome.as_ref())?;
    Ok(())
}

pub fn run_local(
    repository: &GitRepository,
    snapshot: RepositorySnapshot,
    scope: DiffScope,
) -> Result<Option<ReviewSubmission>, TuiError> {
    let (document, archive) = snapshot.into_parts();
    let mut backend = LocalBackend {
        repository,
        scope,
        archive,
        source_responses: Vec::new(),
    };
    run_diff_review(Arc::new(document), &mut backend)
}

trait DiffReviewBackend {
    fn apply(&mut self, action: RepositoryAction) -> Result<DiffDocument, String>;
    fn begin_source_request(&mut self, request: SourceRequest);
    fn drain_source_responses(&mut self) -> Vec<SourceResponse>;
}

fn run_diff_review(
    document: Arc<DiffDocument>,
    backend: &mut dyn DiffReviewBackend,
) -> Result<Option<ReviewSubmission>, TuiError> {
    if !stdin().is_terminal() || !stderr().is_terminal() {
        return Err(TuiError::NoTerminal);
    }

    let _session = TerminalSession::enter()?;
    let terminal_backend = CrosstermBackend::new(stderr());
    let mut terminal = Terminal::new(terminal_backend)?;
    let mut terminal_size = terminal.size()?;
    let mut state = DiffReviewState::with_theme(document, crate::preferences::load_theme());

    loop {
        for request in state.take_source_requests() {
            backend.begin_source_request(request);
        }
        for response in backend.drain_source_responses() {
            state.provide_source(response);
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
        match apply_event(&mut state, event::read()?, backend) {
            EventOutcome::Continue => {}
            EventOutcome::Cancelled => return Ok(None),
            EventOutcome::Submitted(submission) => return Ok(Some(submission)),
        }
        for _ in 1..MAX_COALESCED_EVENTS {
            if !event::poll(Duration::ZERO)? {
                break;
            }
            match apply_event(&mut state, event::read()?, backend) {
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

struct SessionBackend {
    socket_path: PathBuf,
    source_sender: mpsc::Sender<SourceResponse>,
    source_receiver: mpsc::Receiver<SourceResponse>,
}

impl SessionBackend {
    fn new(socket_path: PathBuf) -> Self {
        let (source_sender, source_receiver) = mpsc::channel();
        Self {
            socket_path,
            source_sender,
            source_receiver,
        }
    }

    fn document(&self) -> Result<DiffDocument, TuiError> {
        match self.request(&SessionRequestRef::Document)? {
            SessionResponse::Document(document) => Ok(document),
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
        Self::request_at(&self.socket_path, request)
    }

    fn request_at(
        socket_path: &Path,
        request: &SessionRequestRef<'_>,
    ) -> Result<SessionResponse, TuiError> {
        let mut stream = UnixStream::connect(socket_path)?;
        write_request(&mut stream, request)?;
        let response = read_response(&mut stream)?;
        match response {
            SessionResponse::ProtocolError(message) => Err(TuiError::Protocol(message)),
            response => Ok(response),
        }
    }
}

impl DiffReviewBackend for SessionBackend {
    fn apply(&mut self, action: RepositoryAction) -> Result<DiffDocument, String> {
        match self.request(&SessionRequestRef::RepositoryAction(&action)) {
            Ok(SessionResponse::Document(document)) => Ok(document),
            Ok(SessionResponse::RepositoryError(message)) => Err(message),
            Ok(response) => Err(format!("unexpected review service response: {response:?}")),
            Err(error) => Err(error.to_string()),
        }
    }

    fn begin_source_request(&mut self, request: SourceRequest) {
        let socket_path = self.socket_path.clone();
        let sender = self.source_sender.clone();
        thread::spawn(move || {
            let result = match Self::request_at(&socket_path, &SessionRequestRef::Source(&request))
            {
                Ok(SessionResponse::Source(response)) => response.result,
                Ok(response) => Err(SourceUnavailable::Error(format!(
                    "unexpected review service response: {response:?}"
                ))),
                Err(error) => Err(SourceUnavailable::Error(error.to_string())),
            };
            let response = SourceResponse {
                epoch: request.epoch,
                key: request.key,
                result,
            };
            let _ = sender.send(response);
        });
    }

    fn drain_source_responses(&mut self) -> Vec<SourceResponse> {
        self.source_receiver.try_iter().collect()
    }
}

struct LocalBackend<'a> {
    repository: &'a GitRepository,
    scope: DiffScope,
    archive: SourceArchive,
    source_responses: Vec<SourceResponse>,
}

impl DiffReviewBackend for LocalBackend<'_> {
    fn apply(&mut self, action: RepositoryAction) -> Result<DiffDocument, String> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                execute_repository_action(self.repository, action)
                    .await
                    .map_err(|error| error.to_string())?;
                self.repository
                    .snapshot_with_sources(self.scope)
                    .await
                    .map(|snapshot| {
                        let (document, archive) = snapshot.into_parts();
                        self.archive = archive;
                        document
                    })
                    .map_err(|error| error.to_string())
            })
        })
    }

    fn begin_source_request(&mut self, request: SourceRequest) {
        self.source_responses.push(self.archive.response(&request));
    }

    fn drain_source_responses(&mut self) -> Vec<SourceResponse> {
        std::mem::take(&mut self.source_responses)
    }
}

async fn execute_repository_action(
    repository: &GitRepository,
    action: RepositoryAction,
) -> Result<(), diff_git::GitError> {
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

fn apply_event(
    state: &mut DiffReviewState,
    event: Event,
    backend: &mut dyn DiffReviewBackend,
) -> EventOutcome {
    let previous_theme = state.theme().id().to_string();
    let outcome = match handle_crossterm_event(state, event) {
        Some(DiffReviewEvent::RepositoryAction(action)) => {
            state.set_repository_pending();
            match backend.apply(action) {
                Ok(document) => state.set_document(Arc::new(document)),
                Err(error) => state.set_repository_error(error.clone()),
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
    #[error(transparent)]
    Transport(#[from] crate::protocol::ProtocolError),
    #[error("Markdown review action failed: {0}")]
    MarkdownReview(#[from] diff_markdown::MarkdownReviewError),
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
