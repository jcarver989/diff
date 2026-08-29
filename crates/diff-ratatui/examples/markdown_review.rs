//! Review one rendered Markdown file in a terminal.

use crossterm::{
    event, execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use diff_core::{MarkdownDocument, MarkdownReviewEvent};
use diff_ratatui::{MarkdownReviewState, MarkdownReviewWidget, handle_markdown_crossterm_event};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{env, error::Error, fs, io::stdout, path::Path, sync::Arc};

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> std::io::Result<Self> {
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| "markdown-plan.md".into());
    let source = fs::read_to_string(&path)?;
    let title = Path::new(&path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    let document = MarkdownDocument::parse_with_metadata(Some(path), title, source);
    let outcome = run(Arc::new(document))?;
    if let Some(submission) = outcome {
        println!("{}", submission.formatted);
    }
    Ok(())
}

fn run(
    document: Arc<MarkdownDocument>,
) -> Result<Option<diff_core::MarkdownReviewSubmission>, Box<dyn Error>> {
    let _guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut state = MarkdownReviewState::new(document);
    loop {
        if state.is_dirty() {
            terminal.draw(|frame| {
                frame.render_stateful_widget(MarkdownReviewWidget::new(), frame.area(), &mut state);
                if let Some(position) = state.cursor_position() {
                    frame.set_cursor_position(position);
                }
            })?;
        }
        match handle_markdown_crossterm_event(&mut state, event::read()?)? {
            Some(MarkdownReviewEvent::Submit(submission)) => return Ok(Some(submission)),
            Some(MarkdownReviewEvent::Cancel) => return Ok(None),
            Some(MarkdownReviewEvent::CopyFormatted(_)) | None => {}
        }
    }
}
