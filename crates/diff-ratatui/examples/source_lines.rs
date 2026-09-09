use clankerdiff_markdown::MarkdownDocument;
use clankerdiff_ratatui::{MarkdownLayoutOptions, MarkdownPresentation, MarkdownRenderer};
use clankerdiff_syntax::SyntaxHighlighter;
use clankerdiff_theme::ReviewTheme;

fn main() {
    let document = MarkdownDocument::parse(
        "# Plan\n\n- Preserve source identity\n\n```rust\nlet value = 42;\n```\n",
    );
    let theme = ReviewTheme::default();
    let mut highlighter = SyntaxHighlighter::default();
    for width in [60, 20] {
        let layout = MarkdownRenderer::new().render_layout(
            &document,
            MarkdownLayoutOptions {
                width,
                presentation: MarkdownPresentation::SourceLines,
                ..MarkdownLayoutOptions::default()
            },
            &theme,
            &mut highlighter,
        );
        println!(
            "Width {width}; source line 3 maps to {:?}",
            layout.rows_for_source_line(3)
        );
        for row in layout.rows().iter() {
            println!(
                "{:>3} │ {}",
                row.source.as_ref().map_or(0, |source| source.lines.start),
                row.line
            );
        }
    }
}
