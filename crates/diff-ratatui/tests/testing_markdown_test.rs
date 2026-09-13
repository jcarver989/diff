use clankerdiff_markdown::MarkdownDocument;
use clankerdiff_ratatui::{MarkdownRenderer, testing::MarkdownStreamFixture};
use clankerdiff_syntax::SyntaxHighlighter;

#[test]
fn reference_renders_reparse_without_touching_streaming_highlights() {
    let mut fixture = MarkdownStreamFixture::from_source("```rust\nlet value = 1;\n```\n");
    fixture.layout();
    let streaming_stats = fixture.highlighter.stats();

    let first = fixture.one_shot();
    let first_stats = fixture.reference_highlighter.stats();
    assert!(first_stats.misses > 0);
    assert!(first_stats.bytes > 0);

    let second = fixture.one_shot();
    assert!(first.rows().iter().eq(second.rows().iter()));
    assert_eq!(
        fixture.reference_highlighter.stats().misses,
        2 * first_stats.misses
    );
    assert_eq!(
        fixture.reference_highlighter.stats().bytes,
        2 * first_stats.bytes
    );

    let lines = fixture.one_shot_lines();
    assert_eq!(lines, first.materialize());
    assert_eq!(
        fixture.reference_highlighter.stats().misses,
        3 * first_stats.misses
    );
    assert_eq!(
        fixture.reference_highlighter.stats().bytes,
        3 * first_stats.bytes
    );
    assert_eq!(fixture.reference_highlighter.stats().hits, 0);
    assert_eq!(fixture.highlighter.stats(), streaming_stats);
}

#[test]
fn reused_reference_highlighter_matches_fresh_renders_after_appends() {
    let mut fixture = MarkdownStreamFixture::terminal();
    for chunk in ["Some prose.\n\n```rust\n", "let value = ", "42;\n", "```\n"] {
        fixture.stream.push(chunk);
        fixture.assert_output_equivalent();
        let expected = MarkdownRenderer::new().render_layout(
            &MarkdownDocument::parse(fixture.stream.source()),
            fixture.options,
            &fixture.theme,
            &mut SyntaxHighlighter::default(),
        );
        assert!(fixture.one_shot().rows().iter().eq(expected.rows().iter()));
        assert_eq!(fixture.one_shot_lines(), expected.materialize());
    }
}
