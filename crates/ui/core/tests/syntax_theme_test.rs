use clankerdiff_ui_core::{Fingerprint, FontStyle, ReviewTheme, Rgba, SyntaxTheme};
use rouge_syntax::{DocumentHighlights, analyze};
use std::{error::Error, ops::Range};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn matches_arborium_2_18_2_reference() -> TestResult {
    let expected = [
        "3eea9feacb6bbd5745a9780f1296a1c42550ecc0b071f77c6cbdd32e59b148d0",
        "b03f32d9811b3e965531693cd3bb17e13991a1a101079c2e2ecd246b9ec043b3",
        "73a1c69d82adc986b0a3f3b1955928189638874dd3866950e0985f849ae2987b",
        "936705d041ff09f131a6570c642a00d891b4d48885842d3c050c13e3121be35b",
        "ba5ea32760a709d71661de074ea205f257f439eb6ccdd2e7c1da4639ca20720a",
        "fbe57dd39ed5774a69e712434a080852998feea8096fb131fafaa2b2e46df222",
        "f47a42ca89c15eedcbfd2d994fef9e7154bec30b7c70842cbca905611824ab40",
        "2c262162df46a06dfb78338f4874804ffef1639446bb0e9a13eccf4f1f254e70",
        "437d030e31b6826ef1da11db3b80eef2730aa215a5a8a7a8292cb04b26feec6f",
        "4d457e1bcb37e9fcce7196c6c38b28a1a90d622792df81d753b8ac0f1bba8938",
        "0fa252932afc4f2ff70d5c1aaa8f5a3cc0e3e7bb1b066aafb72f53b80b2acbc9",
        "8ce580eb80739ab2c172b5dbd78ec07aa7123ca3df5b662243addcb772e3fa7b",
        "6947875a52932442300e3a093fbfbfcdee79bfab49be3b2456a82b9da96c2a52",
    ];
    for ((language, source), expected) in [
        (
            "rust",
            "fn main() { let VALUE = Some(123); println!(\"{VALUE}\"); }",
        ),
        (
            "rust",
            "/* open\r\nclosed */\nlet café = r###\"hello\"###;\n\n",
        ),
        (
            "python",
            "class Example:\n    def method(self):\n        return self.VALUE\n",
        ),
        (
            "javascript",
            "const value = /foo/g;\nconsole.log(`hello ${name}`);\n",
        ),
        ("typescript", "const x: number = 1;\nconst y = false;\n"),
        ("tsx", "const view = <Panel title=\"Hi\" />;"),
        ("bash", "echo \"${VALUE:-$(printf '%s' hello)}\"\n"),
        (
            "html",
            "<p>café</p><script>const π = 3.14;</script><style>b{color:red}</style>",
        ),
        ("markdown", "# Title\n\n```rust\nfn main() {}\n```\n"),
        ("markdown", "[label][ref]\n\n[ref]: https://example.com\n"),
        ("yaml", "key: |\n  value\nnext: true\n"),
        ("json", "{\"key\": true, \"nested\": [1, 2]}"),
        ("solidity", "contract Example { struct Pair { uint value; } function run() public { Pair memory p = Pair({value: 1}); } }"),
    ]
    .into_iter()
    .zip(expected)
    {
        let lines = resolved(&analyze(language, source).highlights, &ReviewTheme::sage()?.syntax);
        assert_eq!(
            Fingerprint::of([format!("{lines:?}")]).to_string(),
            expected,
            "{language}: {source:?}: {lines:?}"
        );
    }
    Ok(())
}

#[test]
fn builtin_themes_resolve_the_same_highlights_differently() -> TestResult {
    let highlights = analyze("html", "<script>const café = 1;</script>").highlights;
    let sage = resolved(&highlights, &ReviewTheme::sage()?.syntax);
    let ayu = resolved(&highlights, &ReviewTheme::ayu()?.syntax);
    assert_ne!(sage[0], ayu[0]);
    assert!(sage[0].as_ref().is_some_and(|spans| !spans.is_empty()));
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct HighlightSpan {
    range: Range<usize>,
    foreground: Rgba,
    font_style: FontStyle,
}

fn resolved(highlights: &DocumentHighlights, theme: &SyntaxTheme) -> Vec<Option<Vec<HighlightSpan>>> {
    (0..highlights.line_count())
        .map(|index| {
            highlights.line(index).map(|spans| {
                let mut styled: Vec<HighlightSpan> = Vec::new();
                for span in spans {
                    let Some(style) = theme.highlight(span.highlight) else {
                        continue;
                    };
                    match styled.last_mut() {
                        Some(last)
                            if last.range.end == span.range.start
                                && last.foreground == style.foreground
                                && last.font_style == style.font_style =>
                        {
                            last.range.end = span.range.end;
                        }
                        _ => styled.push(HighlightSpan {
                            range: span.range.clone(),
                            foreground: style.foreground,
                            font_style: style.font_style,
                        }),
                    }
                }
                styled
            })
        })
        .collect()
}
