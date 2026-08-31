use diff_syntax::{
    CacheConfig, HighlightStats, LanguageHint, SourceSequenceId, SyntaxHighlighter, SyntaxStream,
};
use diff_theme::SyntaxTheme;

#[test]
fn stream_preserves_multiline_context_and_utf8_ranges() {
    let mut highlighter = SyntaxHighlighter::default();
    let theme = SyntaxTheme::default();
    let mut stream = SyntaxStream::new("rust");
    let rows = highlighter
        .with_theme(&theme)
        .append(&mut stream, ["fn main() {", "  println!(\"é\");", "}"]);
    assert_eq!(rows.len(), 3);
    for spans in rows {
        for span in spans {
            assert!(span.range.start <= span.range.end);
        }
    }
    let _ = highlighter
        .with_theme(&theme)
        .highlight_source(LanguageHint::Id("rust"), "let x = 1;");
}

#[test]
fn document_highlights_and_take_stats_are_public_contracts() {
    let text = "/* open\nclosed */\n";
    let id = SourceSequenceId::from_lines(text.lines());
    let mut highlighter = SyntaxHighlighter::default();
    let theme = SyntaxTheme::default();
    let highlights = highlighter
        .with_theme(&theme)
        .highlight_document(id, "rust", text);
    assert_eq!(highlights.line_count(), 2);
    assert!(!highlights.line(1).unwrap().is_empty());
    let first = highlighter.take_stats();
    assert_eq!(first.calls, 1);
    assert_eq!(first.misses, 1);
    assert_eq!(first.bytes, text.len());
    assert_eq!(highlighter.take_stats(), HighlightStats::default());
}

#[test]
fn complete_document_is_parsed_once_then_lines_are_constant_time_lookups() {
    let text = (0..10_000).fold(String::new(), |mut text, index| {
        use std::fmt::Write;
        let _ = writeln!(text, "let value_{index} = {index};");
        text
    });
    let id = SourceSequenceId::from_lines(text.lines());
    let theme = SyntaxTheme::default();
    let config = CacheConfig {
        max_entries: 64,
        max_documents: 4,
        max_stream_lines: 32,
    };
    let mut highlighter = SyntaxHighlighter::new(config);
    let highlights = highlighter
        .with_theme(&theme)
        .highlight_document(id, "rust", &text);
    assert!(!highlights.line(9_000).unwrap().is_empty());
    let first_parse = highlighter.take_stats();
    assert_eq!(first_parse.calls, 1);
    assert_eq!(first_parse.misses, 1);
    assert_eq!(first_parse.bytes, text.len());

    for line in [0, 5_000, 9_999] {
        assert!(highlights.line(line).is_some());
    }
    assert_eq!(highlighter.stats(), HighlightStats::default());

    let cached = highlighter
        .with_theme(&theme)
        .highlight_document(id, "rust", &text);
    assert_eq!(cached.line_count(), 10_000);
    assert_eq!(highlighter.stats().calls, 1);
    assert_eq!(highlighter.stats().hits, 1);
    assert_eq!(highlighter.stats().bytes, 0);
}

#[test]
fn speculative_stream_clone_does_not_mutate_stable_continuation() {
    let theme = SyntaxTheme::default();
    let mut highlighter = SyntaxHighlighter::default();
    let mut stable = SyntaxStream::new("rust");
    highlighter
        .with_theme(&theme)
        .append(&mut stable, ["/* stable"]);
    let mut speculative = stable.clone();
    highlighter
        .with_theme(&theme)
        .append(&mut speculative, ["closed */"]);
    let rows = highlighter
        .with_theme(&theme)
        .append(&mut stable, ["still comment"]);
    assert_eq!(rows.len(), 1);
}
