use diff_theme::{FontStyle, Rgba, SyntaxStyle, SyntaxTheme};

#[test]
fn builder_provides_capture_fallback_and_revision() {
    let theme = SyntaxTheme::builder("test")
        .capture(
            "keyword",
            SyntaxStyle {
                foreground: Rgba::new(1, 2, 3, 255),
                font_style: FontStyle::default(),
            },
        )
        .build()
        .unwrap();
    assert_eq!(theme.style("keyword.control"), theme.style("keyword"));
    assert_ne!(theme.revision(), SyntaxTheme::default().revision());
}
