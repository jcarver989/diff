use diff_syntax::{
    HighlightStats, LanguageHint, SequenceLine, SourceSequenceId, SyntaxHighlighter, SyntaxStream,
};
use diff_theme::SyntaxTheme;

#[test]
fn sequence_preserves_multiline_context_and_utf8_ranges() {
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
fn typed_sequences_and_take_stats_are_public_contracts() {
    let lines = ["/* open", "closed */"];
    let id = SourceSequenceId::from_lines(lines);
    let mut highlighter = SyntaxHighlighter::default();
    let theme = SyntaxTheme::default();
    let spans = highlighter
        .with_theme(&theme)
        .highlight_line(SequenceLine::new(id, "rust", 1, lines));
    assert!(!spans.is_empty());
    let first = highlighter.take_stats();
    assert!(first.calls > 0);
    assert_eq!(highlighter.take_stats(), HighlightStats::default());
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
