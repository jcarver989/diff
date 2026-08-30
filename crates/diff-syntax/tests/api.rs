use diff_syntax::{LanguageHint, SyntaxHighlighter, SyntaxSequence};
use diff_theme::SyntaxTheme;

#[test]
fn sequence_preserves_multiline_context_and_utf8_ranges() {
    let mut highlighter = SyntaxHighlighter::default();
    let theme = SyntaxTheme::default();
    let mut sequence = SyntaxSequence::new("rust");
    let rows = highlighter.append_lines(
        &theme,
        &mut sequence,
        ["fn main() {", "  println!(\"é\");", "}"],
    );
    assert_eq!(rows.len(), 3);
    for spans in rows {
        for span in spans {
            assert!(span.range.start <= span.range.end);
        }
    }
    let _ = highlighter.highlight(&theme, LanguageHint::Id("rust"), "let x = 1;");
}
