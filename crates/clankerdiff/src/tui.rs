use crate::protocol::{SessionRequestRef, SessionResponse, read_response, write_request};
use clankerdiff_core::{
    DiffDocument, DiffReviewEvent, DiffScope, RepositoryAction, ReviewSubmission,
};
use clankerdiff_git::GitRepository;
use clankerdiff_markdown::{MarkdownDocument, MarkdownReviewSubmission};
use clankerdiff_ratatui::{
    DiffReviewState, DiffReviewWidget, InputOutcome, MarkdownReviewEvent, MarkdownReviewState,
    MarkdownReviewWidget, ThemeChoice, handle_crossterm_event, handle_markdown_crossterm_event,
};
use clankerdiff_watch::RepositoryWatcher;
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
        mpsc::{self, Receiver, Sender, TryRecvError},
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
    state.set_scope(updates.latest.borrow().scope);
    state.set_theme(crate::preferences::load_theme());
    state.set_theme_choices(ThemeChoice::catalog());
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
    let mut subscription = watcher.state_rx.clone();
    let retained = subscription.borrow_and_update().clone();
    let background_error = retained.error_message();
    let mut installed = retained.snapshot;
    let revision = 1;
    let mut state = DiffReviewState::new(installed.document.clone());
    state.set_scope(installed.scope);
    state.set_theme(crate::preferences::load_theme());
    state.set_theme_choices(ThemeChoice::catalog());
    let initial = HostState {
        revision,
        document: installed.document.clone(),
        scope: installed.scope,
        background_error,
    };
    let (latest, receiver) = watch::channel(initial.clone());
    let mut updates = HostUpdates {
        latest: receiver,
        operation: None,
        installed_revision: revision,
        installed_scope: initial.scope,
    };

    let bridge = runtime.spawn(async move {
        while subscription.changed().await.is_ok() {
            let retained = subscription.borrow_and_update().clone();
            latest.send_modify(|state| {
                state.background_error = retained.error_message();
                if retained.snapshot != installed {
                    state.revision += 1;
                    state.document = retained.snapshot.document.clone();
                    state.scope = retained.snapshot.scope;
                    installed = retained.snapshot;
                }
            });
        }
    });

    let mut backend = LocalBackend {
        repository: repository.clone(),
        requests: Some(watcher.request_tx.clone()),
        runtime,
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
    scope: DiffScope,
    background_error: Option<String>,
}

impl HostState {
    fn scoped(
        revision: u64,
        document: DiffDocument,
        scope: DiffScope,
        background_error: Option<String>,
    ) -> Self {
        Self {
            revision,
            document: Arc::new(document),
            scope,
            background_error,
        }
    }
}

type CommandRx = Receiver<Result<(), String>>;

struct HostUpdates {
    latest: watch::Receiver<HostState>,
    operation: Option<CommandRx>,
    installed_revision: u64,
    installed_scope: DiffScope,
}

impl HostUpdates {
    fn begin(&mut self, state: &mut DiffReviewState, reply: CommandRx) {
        state.set_repository_pending();
        self.operation = Some(reply);
    }

    fn drain(&mut self, state: &mut DiffReviewState) {
        let latest = self.latest.borrow_and_update().clone();
        if latest.revision > self.installed_revision {
            state.set_document(latest.document.clone());
            self.installed_revision = latest.revision;
        }
        if latest.scope != self.installed_scope {
            state.set_scope(latest.scope);
            self.installed_scope = latest.scope;
        }
        state.set_background_error(latest.background_error);
        let Some(operation) = &self.operation else {
            return;
        };
        let result = match operation.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Disconnected) => Err("review command stopped".to_owned()),
            Err(TryRecvError::Empty) => return,
        };
        self.operation = None;
        match result {
            Ok(()) => state.clear_repository_pending(),
            Err(message) => state.set_repository_error(message),
        }
    }
}

trait DiffReviewBackend {
    fn apply(&mut self, action: RepositoryAction) -> CommandRx;
    fn apply_scope(&mut self, scope: DiffScope) -> CommandRx;
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
        match apply_event(&mut state, event::read()?, backend, updates) {
            EventOutcome::Continue => {}
            EventOutcome::Cancelled => return Ok(None),
            EventOutcome::Submitted(submission) => return Ok(Some(submission)),
        }
        for _ in 1..MAX_COALESCED_EVENTS {
            if !event::poll(Duration::ZERO)? {
                break;
            }
            match apply_event(&mut state, event::read()?, backend, updates) {
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
    let outcome = match input.into_event() {
        Some(MarkdownReviewEvent::Submit(submission)) => {
            MarkdownEventOutcome::Submitted(submission)
        }
        Some(MarkdownReviewEvent::Cancel) => MarkdownEventOutcome::Cancelled,
        Some(MarkdownReviewEvent::CopyFormatted(_)) | None => MarkdownEventOutcome::Continue,
    };
    Ok(outcome)
}

enum MarkdownEventOutcome {
    Continue,
    Cancelled,
    Submitted(MarkdownReviewSubmission),
}

enum SessionWork {
    Apply(RepositoryAction, Sender<Result<(), String>>),
    SetScope(DiffScope, Sender<Result<(), String>>),
    Complete(Option<ReviewSubmission>, Sender<Result<(), TuiError>>),
    Stop,
}

/// One owned worker serializes actions, polls and final submission. It tracks
/// fetched revisions, never the render thread's progress.
struct SessionBackend {
    commands: Sender<SessionWork>,
    worker: Option<thread::JoinHandle<()>>,
}

impl SessionBackend {
    fn new(socket_path: PathBuf) -> Result<(Self, HostUpdates), TuiError> {
        let response =
            Self::request_at(&socket_path, &SessionRequestRef::Document { revision: 0 })?;
        let SessionResponse::Document {
            revision,
            document,
            scope,
            background_error,
        } = response
        else {
            return Err(TuiError::UnexpectedResponse(response));
        };
        let (latest, receiver) = watch::channel(HostState::scoped(
            revision,
            document,
            scope,
            background_error,
        ));
        let (commands, requests) = mpsc::channel();
        let worker = thread::spawn(move || {
            loop {
                match requests.recv_timeout(POLL_INTERVAL) {
                    Ok(SessionWork::Apply(action, reply)) => Self::command(
                        &socket_path,
                        &latest,
                        &SessionRequestRef::RepositoryAction(&action),
                        &reply,
                    ),
                    Ok(SessionWork::SetScope(scope, reply)) => Self::command(
                        &socket_path,
                        &latest,
                        &SessionRequestRef::SetScope(scope),
                        &reply,
                    ),
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
                worker: Some(worker),
            },
            HostUpdates {
                latest: receiver,
                operation: None,
                installed_revision: revision,
                installed_scope: scope,
            },
        ))
    }

    fn command(
        socket_path: &Path,
        latest: &watch::Sender<HostState>,
        request: &SessionRequestRef<'_>,
        reply: &Sender<Result<(), String>>,
    ) {
        let result = Self::request_at(socket_path, request)
            .and_then(Self::accepted)
            .map_err(|error| error.to_string());
        Self::poll(socket_path, latest);
        let _ = reply.send(result);
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
                scope,
                background_error,
            }) => {
                latest.send_replace(HostState::scoped(
                    revision,
                    document,
                    scope,
                    background_error,
                ));
                return;
            }
            Ok(SessionResponse::Unchanged {
                scope,
                background_error,
            }) => {
                latest.send_if_modified(|state| {
                    let mut modified = false;
                    if state.scope != scope {
                        state.scope = scope;
                        modified = true;
                    }
                    if state.background_error != background_error {
                        state.background_error.clone_from(&background_error);
                        modified = true;
                    }
                    modified
                });
                return;
            }
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
    fn apply(&mut self, action: RepositoryAction) -> CommandRx {
        let (reply, receiver) = mpsc::channel();
        let _ = self.commands.send(SessionWork::Apply(action, reply));
        receiver
    }

    fn apply_scope(&mut self, scope: DiffScope) -> CommandRx {
        let (reply, receiver) = mpsc::channel();
        let _ = self.commands.send(SessionWork::SetScope(scope, reply));
        receiver
    }
}

struct LocalBackend {
    repository: GitRepository,
    requests: Option<tokio::sync::mpsc::Sender<clankerdiff_watch::RepositoryRequest>>,
    runtime: Handle,
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
    fn apply(&mut self, action: RepositoryAction) -> CommandRx {
        let repository = self.repository.clone();
        let (completed, reply) = mpsc::channel();
        self.task = Some(self.runtime.spawn(async move {
            let result = repository
                .apply(action)
                .await
                .map_err(|error| error.to_string());
            let _ = completed.send(result);
        }));
        reply
    }

    fn apply_scope(&mut self, scope: DiffScope) -> CommandRx {
        let (completed, reply) = mpsc::channel();
        let Some(requests) = self.requests.clone() else {
            let _ = completed.send(Err("review session stopped".to_owned()));
            return reply;
        };
        self.task = Some(self.runtime.spawn(async move {
            let (result_tx, completion) = tokio::sync::oneshot::channel();
            let mut result = requests
                .send(clankerdiff_watch::RepositoryRequest::SetScope { scope, result_tx })
                .await
                .map_err(|_| "review session stopped".to_owned());
            if result.is_ok() {
                result = completion
                    .await
                    .map_err(|_| "review session stopped".to_owned())
                    .map_err(|error| error.clone())
                    .and_then(|inner| inner.map_err(|error| error.to_string()));
            }
            let _ = completed.send(result);
        }));
        reply
    }
}

fn apply_event(
    state: &mut DiffReviewState,
    event: Event,
    backend: &mut dyn DiffReviewBackend,
    updates: &mut HostUpdates,
) -> EventOutcome {
    let input = handle_crossterm_event(state, event);
    if let InputOutcome::ThemeSelected(id) = &input {
        let _ = crate::preferences::save_theme(&id.to_string());
    }
    let command = match input.into_event() {
        Some(DiffReviewEvent::Cancel) => return EventOutcome::Cancelled,
        Some(DiffReviewEvent::SubmitReview(submission)) => {
            return EventOutcome::Submitted(submission);
        }
        Some(DiffReviewEvent::CopyFormattedReview(_)) | None => return EventOutcome::Continue,
        Some(command) => command,
    };
    if state.repository_pending() {
        return EventOutcome::Continue;
    }
    let reply = match command {
        DiffReviewEvent::Refresh => backend.apply_scope(state.scope()),
        DiffReviewEvent::SetScope(scope) => backend.apply_scope(scope),
        DiffReviewEvent::RepositoryAction(action) => backend.apply(action),
        DiffReviewEvent::Cancel
        | DiffReviewEvent::SubmitReview(_)
        | DiffReviewEvent::CopyFormattedReview(_) => return EventOutcome::Continue,
    };
    updates.begin(state, reply);
    EventOutcome::Continue
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
    MarkdownReview(#[from] clankerdiff_markdown::MarkdownReviewError),
    #[error("terminal or review service I/O failed: {0}")]
    Io(#[from] io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_value_updates_preserve_pending_and_reconcile_health() {
        let initial = HostState::scoped(
            1,
            DiffDocument::empty(),
            clankerdiff_core::DiffScope::Both,
            None,
        );
        let mut state = DiffReviewState::new(initial.document.clone());
        state.set_scope(initial.scope);
        let (latest, receiver) = watch::channel(initial.clone());
        let mut updates = HostUpdates {
            latest: receiver,
            operation: None,
            installed_revision: 1,
            installed_scope: initial.scope,
        };
        let (completed, reply) = mpsc::channel();
        updates.begin(&mut state, reply);
        for revision in 2..=100 {
            latest.send_replace(HostState::scoped(
                revision,
                DiffDocument::empty(),
                clankerdiff_core::DiffScope::Both,
                None,
            ));
        }
        assert_eq!(state.scope(), DiffScope::Both);
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
        latest.send_replace(HostState::scoped(
            99,
            DiffDocument::empty(),
            clankerdiff_core::DiffScope::Both,
            None,
        ));
        updates.drain(&mut state);
        assert!(state.repository_pending());
        assert_eq!(updates.installed_revision, 100);
        completed.send(Ok(())).unwrap();
        updates.drain(&mut state);
        assert!(!state.repository_pending());
        assert!(completed.send(Err("stale".into())).is_err());
        updates.drain(&mut state);
        assert_eq!(state.repository_error(), None);
        let (completed, reply) = mpsc::channel();
        updates.begin(&mut state, reply);
        completed.send(Err("command failed".into())).unwrap();
        updates.drain(&mut state);
        latest.send_modify(|state| state.background_error = None);
        updates.drain(&mut state);
        assert_eq!(state.repository_error(), Some("command failed"));
    }

    #[test]
    fn a_dropped_command_settles_as_an_error() {
        let initial = HostState::scoped(
            1,
            DiffDocument::empty(),
            clankerdiff_core::DiffScope::Both,
            None,
        );
        let mut state = DiffReviewState::new(initial.document.clone());
        let (_latest, receiver) = watch::channel(initial);
        let mut updates = HostUpdates {
            latest: receiver,
            operation: None,
            installed_revision: 1,
            installed_scope: DiffScope::Both,
        };
        let (completed, reply) = mpsc::channel();
        updates.begin(&mut state, reply);
        drop(completed);
        updates.drain(&mut state);
        assert!(!state.repository_pending());
        assert_eq!(state.repository_error(), Some("review command stopped"));
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
