use clankerdiff_markdown::{MarkdownDocument, MarkdownStream};
use clankerdiff_ratatui::{MarkdownLayoutOptions, MarkdownRenderer, StreamingMarkdownState};
use clankerdiff_ui_core::ReviewTheme;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let renderer = MarkdownRenderer::new();
    let options = MarkdownLayoutOptions::default();
    let theme = ReviewTheme::default();
    let mut stream = MarkdownStream::new();
    let mut state = StreamingMarkdownState::default();

    for chunk in ["Answer:\n\n```rust\n", "fn main() {\n", "}\n```\n"] {
        stream.push(chunk);
        let lines = renderer.render_stream_lines(&mut state, &stream, options, &theme)?;
        println!("revision {}: {} rows", stream.revision(), lines.len());
    }
    stream.finish();
    let streamed = renderer.render_stream_lines(&mut state, &stream, options, &theme)?;
    let one_shot = renderer.render_lines(&MarkdownDocument::parse(stream.source()), options, &theme);
    assert_eq!(streamed, one_shot);
    println!("markdown: {:?}", state.take_stats());

    stream.replace("Replacement.\n\n");
    renderer.render_stream_lines(&mut state, &stream, options, &theme)?;
    Ok(())
}
