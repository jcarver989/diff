use crate::protocol::{SessionRequestRef, SessionResponse, read_response, write_request};
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use diff_core::{DiffDocument, RepositoryAction, ReviewSubmission};
use diff_git::{GitError, GitRepository};
use diff_markdown::{MarkdownDocument, MarkdownReviewSubmission};
use diff_ratatui::{
    DiffReviewEvent, DiffReviewState, DiffReviewWidget, MarkdownReviewEvent, MarkdownReviewState,
    MarkdownReviewWidget, handle_crossterm_event, handle_markdown_crossterm_event,
};
use diff_watch::RepositoryWatcher;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    env,
    io::{self, IsTerminal, Write, stderr, stdin},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};
use thiserror::Error;
use tokio::{runtime::Handle, sync::watch, task::JoinHandle};

pub const COMMAND_ENV: &str = "CLANKERDIFF_TUI_COMMAND";

const COMMAND_PLACEHOLDER: &str = "{command}";
const MAX_COALESCED_EVENTS: usize = 256;
/// How often an attached client asks the review service for a newer document.
const POLL_INTERVAL: Duration = Duration::from_millis(250);
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
    let (mut backend, mut updates) = SessionBackend::new(socket_path)?;
    let mut state = DiffReviewState::new(updates.latest.borrow().document.clone());
    state.set_theme(crate::preferences::load_theme());
    let outcome = run_diff_review(state, &mut backend, &mut updates)?;
    backend.complete(outcome)?;
    Ok(())
}

/// Reviews in this terminal, applying watcher snapshots as they are published.
pub fn run_local(
    repository: &GitRepository,
    watcher: &RepositoryWatcher,
) -> Result<Option<ReviewSubmission>, TuiError> {
    let runtime = Handle::current();
    let mut subscription = watcher.snapshot_rx.clone();
    let mut installed = subscription
        .borrow_and_update()
        .clone()
        .map_err(TuiError::Git)?;
    let revision = 1;
    let document = installed.document.clone();
    let mut state = DiffReviewState::new(document.clone());
    state.set_theme(crate::preferences::load_theme());
    let initial = HostState {
        revision,
        document,
        background_error: None,
    };
    let (latest, receiver) = watch::channel(initial);
    let (completed, commands) = mpsc::channel();
    let mut updates = HostUpdates {
        latest: receiver,
        commands,
        installed_revision: revision,
    };

    let bridge = runtime.spawn(async move {
        while subscription.changed().await.is_ok() {
            let result = subscription.borrow_and_update().clone();
            latest.send_modify(|state| match result {
                Ok(snapshot) => {
                    if snapshot != installed {
                        state.revision += 1;
                        state.document = snapshot.document.clone();
                        installed = snapshot;
                    }
                    state.background_error = None;
                }
                Err(error) => state.background_error = Some(error.to_string()),
            });
        }
    });

    let mut backend = LocalBackend {
        repository: repository.clone(),
        runtime,
        completed,
        task: None,
    };

    let outcome =
        tokio::task::block_in_place(|| run_diff_review(state, &mut backend, &mut updates));
    bridge.abort();
    outcome
}

/// Content and health are latest-value state; command results are not.
#[derive(Clone)]
struct HostState {
    revision: u64,
    document: Arc<DiffDocument>,
    background_error: Option<String>,
}

impl HostState {
    fn document(revision: u64, document: DiffDocument, background_error: Option<String>) -> Self {
        Self {
            revision,
            document: Arc::new(document),
            background_error,
        }
    }
}

struct HostUpdates {
    latest: watch::Receiver<HostState>,
    commands: Receiver<Result<(), String>>,
    installed_revision: u64,
}

impl HostUpdates {
    fn drain(&mut self, state: &mut DiffReviewState) {
        let latest = self.latest.borrow_and_update().clone();
        if latest.revision > self.installed_revision {
            state.set_document(latest.document.clone());
            self.installed_revision = latest.revision;
        }
        state.set_background_error(latest.background_error);
        // Hosts allow one command at a time. Only its reply can settle pending.
        match self.commands.try_recv() {
            Ok(Ok(())) => state.clear_repository_pending(),
            Ok(Err(message)) => state.set_repository_error(message),
            Err(_) => {}
        }
    }
}

trait DiffReviewBackend {
    fn apply(&mut self, action: RepositoryAction);
}

fn run_diff_review(
    mut state: DiffReviewState,
    backend: &mut dyn DiffReviewBackend,
    updates: &mut HostUpdates,
) -> Result<Option<ReviewSubmission>, TuiError> {
    if !stdin().is_terminal() || !stderr().is_terminal() {
        return Err(TuiError::NoTerminal);
    }

    let _session = TerminalSession::enter()?;
    let terminal_backend = CrosstermBackend::new(stderr());
    let mut terminal = Terminal::new(terminal_backend)?;
    let mut terminal_size = terminal.size()?;

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
            updates.drain(&mut state);
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
        updates.drain(&mut state);
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

enum SessionWork {
    Apply(RepositoryAction),
    Complete(Option<ReviewSubmission>, Sender<Result<(), TuiError>>),
    Stop,
}

/// One owned worker serializes actions, polls and final submission. It tracks
/// fetched revisions, never the render thread's progress.
struct SessionBackend {
    commands: Sender<SessionWork>,
    completed: Sender<Result<(), String>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl SessionBackend {
    fn new(socket_path: PathBuf) -> Result<(Self, HostUpdates), TuiError> {
        let response =
            Self::request_at(&socket_path, &SessionRequestRef::Document { revision: 0 })?;
        let SessionResponse::Document {
            revision,
            document,
            background_error,
        } = response
        else {
            return Err(TuiError::UnexpectedResponse(response));
        };
        let (latest, receiver) =
            watch::channel(HostState::document(revision, document, background_error));
        let (completed, results) = mpsc::channel();
        let (commands, requests) = mpsc::channel();
        let worker_completed = completed.clone();
        let worker = thread::spawn(move || {
            loop {
                match requests.recv_timeout(POLL_INTERVAL) {
                    Ok(SessionWork::Apply(action)) => {
                        let result = Self::request_at(
                            &socket_path,
                            &SessionRequestRef::RepositoryAction(&action),
                        )
                        .and_then(Self::accepted)
                        .map_err(|error| error.to_string());
                        Self::poll(&socket_path, &latest);
                        let _ = worker_completed.send(result);
                    }
                    Ok(SessionWork::Complete(submission, reply)) => {
                        let request = match &submission {
                            Some(submission) => SessionRequestRef::Submit(submission),
                            None => SessionRequestRef::Cancel,
                        };
                        let _ = reply.send(
                            Self::request_at(&socket_path, &request).and_then(Self::accepted),
                        );
                        return;
                    }
                    Ok(SessionWork::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    Err(mpsc::RecvTimeoutError::Timeout) => Self::poll(&socket_path, &latest),
                }
            }
        });
        Ok((
            Self {
                commands,
                completed,
                worker: Some(worker),
            },
            HostUpdates {
                latest: receiver,
                commands: results,
                installed_revision: revision,
            },
        ))
    }

    fn accepted(response: SessionResponse) -> Result<(), TuiError> {
        match response {
            SessionResponse::Accepted => Ok(()),
            SessionResponse::RepositoryError(message) => Err(TuiError::Protocol(message)),
            response => Err(TuiError::UnexpectedResponse(response)),
        }
    }

    fn poll(socket_path: &Path, latest: &watch::Sender<HostState>) {
        let revision = latest.borrow().revision;
        let response = Self::request_at(socket_path, &SessionRequestRef::Document { revision });
        let background_error = match response {
            Ok(SessionResponse::Document {
                revision,
                document,
                background_error,
            }) => {
                latest.send_replace(HostState::document(revision, document, background_error));
                return;
            }
            Ok(SessionResponse::Unchanged { background_error }) => background_error,
            Ok(response) => Some(format!("unexpected review service response: {response:?}")),
            Err(error) => Some(error.to_string()),
        };

        latest.send_if_modified(|state| {
            if state.background_error == background_error {
                return false;
            }
            state.background_error = background_error;
            true
        });
    }

    fn complete(&self, submission: Option<ReviewSubmission>) -> Result<(), TuiError> {
        let (reply, result) = mpsc::channel();
        self.commands
            .send(SessionWork::Complete(submission, reply))
            .map_err(|_| TuiError::Protocol("session worker stopped".to_owned()))?;
        result
            .recv()
            .map_err(|_| TuiError::Protocol("session worker stopped".to_owned()))?
    }

    fn request_at(
        socket_path: &Path,
        request: &SessionRequestRef<'_>,
    ) -> Result<SessionResponse, TuiError> {
        let mut stream = UnixStream::connect(socket_path)?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.set_write_timeout(Some(Duration::from_secs(30)))?;
        write_request(&mut stream, request)?;
        match read_response(&mut stream)? {
            SessionResponse::ProtocolError(message) => Err(TuiError::Protocol(message)),
            response => Ok(response),
        }
    }
}

impl Drop for SessionBackend {
    fn drop(&mut self) {
        let _ = self.commands.send(SessionWork::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl DiffReviewBackend for SessionBackend {
    fn apply(&mut self, action: RepositoryAction) {
        if self.commands.send(SessionWork::Apply(action)).is_err() {
            let _ = self
                .completed
                .send(Err("session worker stopped".to_owned()));
        }
    }
}

struct LocalBackend {
    repository: GitRepository,
    runtime: Handle,
    completed: Sender<Result<(), String>>,
    task: Option<JoinHandle<()>>,
}

impl Drop for LocalBackend {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

impl DiffReviewBackend for LocalBackend {
    fn apply(&mut self, action: RepositoryAction) {
        let repository = self.repository.clone();
        let completed = self.completed.clone();
        self.task = Some(self.runtime.spawn(async move {
            let result = repository
                .apply(action)
                .await
                .map_err(|error| error.to_string());
            let _ = completed.send(result);
        }));
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
            if state.repository_pending() {
                return EventOutcome::Continue;
            }
            state.set_repository_pending();
            backend.apply(action);
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
    #[error(transparent)]
    Git(Arc<GitError>),
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
    fn latest_value_updates_preserve_pending_and_reconcile_health() {
        let initial = HostState::document(1, DiffDocument::empty(), None);
        let mut state = DiffReviewState::new(initial.document.clone());
        let (latest, receiver) = watch::channel(initial);
        let (completed, commands) = mpsc::channel();
        let mut updates = HostUpdates {
            latest: receiver,
            commands,
            installed_revision: 1,
        };
        state.set_repository_pending();
        for revision in 2..=100 {
            latest.send_replace(HostState::document(revision, DiffDocument::empty(), None));
        }
        latest.send_modify(|state| state.background_error = Some("refresh failed".into()));
        updates.drain(&mut state);
        assert_eq!(updates.installed_revision, 100);
        assert!(state.repository_pending());
        assert_eq!(state.repository_error(), Some("refresh failed"));
        latest.send_modify(|state| state.background_error = None);
        updates.drain(&mut state);
        assert!(state.repository_pending());
        assert_eq!(state.repository_error(), None);
        // Equal/stale content cannot finish a command either.
        latest.send_replace(HostState::document(99, DiffDocument::empty(), None));
        updates.drain(&mut state);
        assert!(state.repository_pending());
        assert_eq!(updates.installed_revision, 100);
        completed.send(Ok(())).unwrap();
        updates.drain(&mut state);
        assert!(!state.repository_pending());
        completed.send(Err("command failed".into())).unwrap();
        updates.drain(&mut state);
        latest.send_modify(|state| state.background_error = None);
        updates.drain(&mut state);
        assert_eq!(state.repository_error(), Some("command failed"));
    }

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
