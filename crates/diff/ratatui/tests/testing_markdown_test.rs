use clankerdiff_markdown::MarkdownDocument;
use clankerdiff_ratatui::{MarkdownRenderStats, MarkdownRenderer, testing::MarkdownStreamFixture};

#[test]
fn reference_renders_leave_the_streaming_state_untouched() {
    let mut fixture = MarkdownStreamFixture::from_source("```rust\nlet value = 1;\n```\n");
    let streamed = fixture.layout();
    fixture.state.take_stats();

    let first = fixture.one_shot();
    let second = fixture.one_shot();
    assert!(first.rows().iter().eq(second.rows().iter()));
    assert!(first.rows().iter().map(|row| &row.line).eq(streamed.rows().iter().map(|row| &row.line)));
    assert_eq!(fixture.one_shot_lines(), first.materialize());
    assert_eq!(fixture.state.take_stats(), MarkdownRenderStats::default());
}

#[test]
fn reference_renders_match_fresh_renders_after_appends() {
    let mut fixture = MarkdownStreamFixture::terminal();
    for chunk in ["Some prose.\n\n```rust\n", "let value = ", "42;\n", "```\n"] {
        fixture.stream.push(chunk);
        fixture.assert_output_equivalent();
        let expected = MarkdownRenderer::new().render_layout(
            &MarkdownDocument::parse(fixture.stream.source()),
            fixture.options,
            &fixture.theme,
        );
        assert!(fixture.one_shot().rows().iter().eq(expected.rows().iter()));
        assert_eq!(fixture.one_shot_lines(), expected.materialize());
    }
}
