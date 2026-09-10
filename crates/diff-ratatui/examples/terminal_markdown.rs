use clankerdiff_markdown::MarkdownStream;
use clankerdiff_ratatui::{
    MarkdownCommitError, MarkdownLayoutOptions, MarkdownRenderer, MarkdownRow,
    StreamingMarkdownPolicy, StreamingMarkdownState,
};
use clankerdiff_syntax::SyntaxHighlighter;
use clankerdiff_theme::{ReviewTheme, Rgba};
use std::{error::Error, sync::Arc};

const VIEWPORT_ROWS: usize = 4;

struct NativeHistory {
    rows: Vec<String>,
    fail_next_write: bool,
}

impl NativeHistory {
    fn write(&mut self, rows: &[Arc<MarkdownRow>]) -> Result<(), String> {
        if std::mem::take(&mut self.fail_next_write) {
            return Err("terminal write failed".to_owned());
        }
        self.rows
            .extend(rows.iter().map(|row| row.line.to_string()));
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let renderer = MarkdownRenderer::new();
    let options = MarkdownLayoutOptions {
        width: 40,
        ..MarkdownLayoutOptions::default()
    };
    let mut theme = ReviewTheme::default();
    let mut highlighter = SyntaxHighlighter::default();
    let mut stream = MarkdownStream::new();
    let mut state = StreamingMarkdownState::new(StreamingMarkdownPolicy::Terminal);
    let mut history = NativeHistory {
        rows: Vec::new(),
        fail_next_write: false,
    };
    let mut live: Vec<Arc<MarkdownRow>> = Vec::new();
    let mut revision = 0;

    let chunks = [
        "Streaming contract\n",
        "===\n\n",
        "The heading above was a plain paragraph until its underline arrived.\n\n",
        "```rust\nfn main() {\n",
        "    println!(\"hello\");\n",
        "}\n```\n\n",
        "See [the docs] for details.\n\n",
        "[the docs]: https://example.com\n",
    ];
    for (index, chunk) in chunks.iter().enumerate() {
        stream.push(chunk);
        renderer.render_stream_layout(&mut state, &stream, options, &theme, &mut highlighter)?;
        let update = state.update_since(revision);
        live.truncate(update.first_changed_row - state.committed_rows());
        live.extend(update.replacement.iter().cloned());
        revision = update.revision;

        history.fail_next_write = index == 3;
        let overflow = live.len().saturating_sub(VIEWPORT_ROWS);
        let pending = overflow.min(live.len().saturating_sub(1));
        if pending > 0 {
            match history.write(&live[..pending]) {
                Ok(()) => {
                    let end = state.committed_rows() + pending;
                    state.commit_rows(revision, end)?;
                    live.drain(..pending);
                }
                Err(error) => println!("kept {pending} rows live: {error}"),
            }
        }
        println!(
            "revision {revision}: {} committed, {} live",
            state.committed_rows(),
            live.len()
        );
    }

    stream.finish();
    theme.markdown.heading = Rgba::new(200, 120, 40, 255);
    renderer.render_stream_layout(&mut state, &stream, options, &theme, &mut highlighter)?;
    let update = state.update_since(revision);
    assert!(update.first_changed_row >= state.committed_rows());
    live.truncate(update.first_changed_row - state.committed_rows());
    live.extend(update.replacement.iter().cloned());

    match state.commit_rows(update.revision.saturating_sub(1), 1) {
        Err(MarkdownCommitError::StaleRevision { .. }) => println!("stale commit rejected"),
        other => println!("unexpected commit result: {other:?}"),
    }

    println!("--- native history ---");
    for row in &history.rows {
        println!("{row}");
    }
    println!("--- live region ---");
    for row in &live {
        println!("{}", row.line);
    }
    println!("{:?}", state.take_stats());
    Ok(())
}
