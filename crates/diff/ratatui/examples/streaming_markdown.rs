use clankerdiff_markdown::{MarkdownDocument, MarkdownStream};
use clankerdiff_ratatui::{MarkdownLayoutOptions, MarkdownRenderer, StreamingMarkdownState};
use clankerdiff_syntax::SyntaxHighlighter;
use clankerdiff_theme::ReviewTheme;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let renderer = MarkdownRenderer::new();
    let options = MarkdownLayoutOptions::default();
    let theme = ReviewTheme::default();
    let mut highlighter = SyntaxHighlighter::default();
    let mut stream = MarkdownStream::new();
    let mut state = StreamingMarkdownState::default();

    for chunk in ["Answer:\n\n```rust\n", "fn main() {\n", "}\n```\n"] {
        stream.push(chunk);
        let lines =
            renderer.render_stream_lines(&mut state, &stream, options, &theme, &mut highlighter)?;
        println!("revision {}: {} rows", stream.revision(), lines.len());
    }
    stream.finish();
    let streamed =
        renderer.render_stream_lines(&mut state, &stream, options, &theme, &mut highlighter)?;
    let one_shot = renderer.render_lines(
        &MarkdownDocument::parse(stream.source()),
        options,
        &theme,
        &mut highlighter,
    );
    assert_eq!(streamed, one_shot);
    println!("markdown: {:?}", state.take_stats());
    println!("syntax: {:?}", highlighter.take_stats());

    stream.replace("Replacement.\n\n");
    renderer.render_stream_lines(&mut state, &stream, options, &theme, &mut highlighter)?;
    Ok(())
}
