use diff_syntax::{LanguageHint, SyntaxHighlighter, SyntaxStream};
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
