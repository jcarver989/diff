use clankerdiff_core::FileDiff;
use clankerdiff_markdown::MarkdownStream;
use clankerdiff_ratatui::{
    DiffPreviewOptions, DiffPreviewState, MarkdownLayoutOptions, MarkdownRenderer,
    StreamingMarkdownState,
};
use clankerdiff_syntax::SyntaxHighlighter;
use clankerdiff_theme::ReviewTheme;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let theme = ReviewTheme::default();
    let mut highlighter = SyntaxHighlighter::default();
    let renderer = MarkdownRenderer::new();
    let mut stream = MarkdownStream::new();
    let mut state = StreamingMarkdownState::default();
    let mut rows = Vec::new();
    let mut revision = 0;
    for chunk in [
        "# Update\n\n",
        "Use the new value:\n\n```rust\n",
        "let value = 42;\n```\n",
    ] {
        stream.push(chunk);
        renderer.render_stream_layout(
            &mut state,
            &stream,
            MarkdownLayoutOptions::default(),
            &theme,
            &mut highlighter,
        )?;
        let update = state.update_since(revision);
        rows.truncate(update.first_changed_row);
        rows.extend(update.replacement.iter().cloned());
        revision = update.revision;
    }
    stream.finish();
    renderer.render_stream_layout(
        &mut state,
        &stream,
        MarkdownLayoutOptions::default(),
        &theme,
        &mut highlighter,
    )?;
    let update = state.update_since(revision);
    rows.truncate(update.first_changed_row);
    rows.extend(update.replacement.iter().cloned());
    for row in rows {
        println!("{}", row.line);
    }
    let mut preview = DiffPreviewState::new(FileDiff::from_texts(
        "main.rs",
        "let value = 1;\n",
        "let value = 42;\n",
    )?);
    for row in preview
        .render(80, &theme, &mut highlighter, DiffPreviewOptions::default())
        .iter()
    {
        println!("{row}");
    }
    Ok(())
}
