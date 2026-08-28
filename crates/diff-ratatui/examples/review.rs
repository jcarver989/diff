//! Interactive review of a real Git worktree using `diff-ratatui`.

use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use diff_core::{DiffDocument, DiffScope, ReviewSubmission};
use diff_git::GitRepository;
use diff_ratatui::{DiffReviewEvent, DiffReviewState, DiffReviewWidget, handle_crossterm_event};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    env,
    error::Error,
    io::{self, Write, stdout},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

const USAGE: &str = "Usage: cargo run -p diff-ratatui --example review -- [PATH] [--scope SCOPE]\n\nScopes:\n  both       staged and unstaged changes (default)\n  unstaged   worktree changes only\n  staged     index changes only\n\nKeys:\n  j/k        move through files or lines\n  h/l, Tab   switch between file and diff panes\n  c          add a comment to the selected line\n  e/x/u      edit, delete, or undo a comment\n  v          cycle automatic, unified, and split views\n  s          submit the review and print it\n  y          copy event (printed after leaving the TUI)\n  ?          show all shortcuts\n  Esc        exit";

#[derive(Debug)]
struct Options {
    path: PathBuf,
    scope: DiffScope,
    help: bool,
}

#[derive(Debug)]
enum Outcome {
    Cancelled,
    Submitted(ReviewSubmission),
    CopyRequested(String),
}

/// Events applied between two frames. A trackpad flick queues far more wheel
/// notches than there are frames worth drawing, so the loop drains what the
/// terminal already has before it draws once.
const MAX_COALESCED_EVENTS: usize = 256;

/// Button presses (`?1000h`) reported as SGR (`?1006h`), and nothing else.
///
/// Crossterm's `EnableMouseCapture` also asks for `?1002h`/`?1003h`, which
/// report every pointer step. That is one event, and one frame, per pixel of
/// mouse movement, for input the widget ignores.
const ENABLE_MOUSE_BUTTONS: &[u8] = b"\x1b[?1000h\x1b[?1006h";
const DISABLE_MOUSE_BUTTONS: &[u8] = b"\x1b[?1006l\x1b[?1000l";

struct TerminalSession;

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        if let Err(error) = write_all_flushed(ENABLE_MOUSE_BUTTONS) {
            let _ = execute!(stdout(), LeaveAlternateScreen);
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
        let _ = execute!(stdout(), LeaveAlternateScreen);
    }
}

fn write_all_flushed(bytes: &[u8]) -> io::Result<()> {
    let mut out = stdout();
    out.write_all(bytes)?;
    out.flush()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let options = parse_options(env::args().skip(1))?;
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }

    let repository = GitRepository::discover(&options.path).await?;
    let document = repository.snapshot(options.scope).await?;
    let outcome = run_tui(Arc::new(document))?;

    match outcome {
        Outcome::Cancelled => {}
        Outcome::Submitted(submission) => println!("{}", submission.formatted),
        Outcome::CopyRequested(formatted) => {
            println!("CopyFormattedReview event:\n\n{formatted}");
        }
    }
    Ok(())
}

fn run_tui(document: Arc<DiffDocument>) -> io::Result<Outcome> {
    let _session = TerminalSession::enter()?;
    let backend = CrosstermBackend::new(stdout());
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

        // Block for the next event, then apply everything already queued behind
        // it. Drawing once per event instead makes a burst of wheel notches one
        // full repaint each, which is more than a terminal can absorb.
        if let Some(outcome) = apply_event(&mut state, event::read()?) {
            return Ok(outcome);
        }
        for _ in 1..MAX_COALESCED_EVENTS {
            if !event::poll(Duration::ZERO)? {
                break;
            }
            if let Some(outcome) = apply_event(&mut state, event::read()?) {
                return Ok(outcome);
            }
        }
    }
}

fn apply_event(state: &mut DiffReviewState, event: Event) -> Option<Outcome> {
    handle_crossterm_event(state, event).map(|event| match event {
        DiffReviewEvent::Cancel => Outcome::Cancelled,
        DiffReviewEvent::SubmitReview(submission) => Outcome::Submitted(submission),
        DiffReviewEvent::CopyFormattedReview(formatted) => Outcome::CopyRequested(formatted),
    })
}

fn parse_options(args: impl IntoIterator<Item = String>) -> io::Result<Options> {
    let mut path = None;
    let mut scope = DiffScope::Both;
    let mut help = false;
    let mut args = args.into_iter();

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "-h" | "--help" => help = true,
            "--scope" => {
                let value = args
                    .next()
                    .ok_or_else(|| invalid_input(&"--scope needs a value"))?;
                scope = value.parse().map_err(|error| invalid_input(&error))?;
            }
            value if value.starts_with('-') => {
                return Err(invalid_input(&format!("unknown option: {value}")));
            }
            value if path.is_none() => path = Some(PathBuf::from(value)),
            value => return Err(invalid_input(&format!("unexpected argument: {value}"))),
        }
    }

    Ok(Options {
        path: path.unwrap_or_else(|| PathBuf::from(".")),
        scope,
        help,
    })
}

fn invalid_input(message: &impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_defaults_and_explicit_scope() {
        let defaults = parse_options(Vec::new()).expect("default options");
        assert_eq!(defaults.path, PathBuf::from("."));
        assert_eq!(defaults.scope, DiffScope::Both);

        let explicit = parse_options([
            "/repo".to_owned(),
            "--scope".to_owned(),
            "staged".to_owned(),
        ])
        .expect("explicit options");
        assert_eq!(explicit.path, PathBuf::from("/repo"));
        assert_eq!(explicit.scope, DiffScope::Staged);
    }

    #[test]
    fn rejects_invalid_arguments() {
        assert!(parse_options(["--scope".to_owned()]).is_err());
        assert!(parse_options(["--scope".to_owned(), "unknown".to_owned()]).is_err());
        assert!(parse_options(["--wat".to_owned()]).is_err());
        assert!(parse_options(["one".to_owned(), "two".to_owned()]).is_err());
    }
}
