mod support;

use ast_grep_core::{
    AstGrep, Language, Pattern, PatternError,
    matcher::PatternBuilder,
    tree_sitter::{LanguageExt, StrDoc, TSLanguage},
};
use clankerdiff_syntax::{
    DocumentHighlights, Fingerprint, FontStyle, LanguageHint, MAX_SOURCE_BYTES, Rgba, SymbolTags, SyntaxError,
    SyntaxTheme, analyze, thread_work,
};
use clankerdiff_ui_core::ReviewTheme;
use std::{fmt::Write, num::NonZeroU16, ops::Range};
use support::{SyntaxBuilder, TestResult, line};

#[test]
fn shares_upstream_runtime_with_ast_grep() -> TestResult {
    let source = "fn main() {}";
    let highlights = analyze("rust", source).highlights;
    assert!(!line(&highlights, 0)?.is_empty());
    let document = StrDoc::try_new(source, Rust)?;
    let ast = AstGrep::doc(document);
    let pattern = Pattern::try_new(source, Rust)?;
    let root = ast.root();
    let found = root.find(&pattern).ok_or("ast-grep did not match Rust function")?;
    assert_eq!(found.text(), source);
    Ok(())
}

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
        let mut fixture = SyntaxBuilder::default().language(language).build();
        fixture.append(source)?;
        fixture.assert_equivalent();
        let lines = resolved(fixture.stream.highlights(), &SyntaxTheme::default());
        assert_eq!(
            Fingerprint::of([format!("{lines:?}")]).to_string(),
            expected,
            "{language}: {source:?}: {lines:?}"
        );
    }
    Ok(())
}

#[test]
fn complete_input_and_first_append_produce_identical_highlights() -> TestResult {
    let mut fixture = SyntaxBuilder::default().build();
    let source = "/* open\nclosed */\nlet café = 1;\n";
    let document = fixture.highlight(source);
    let update = fixture.append(source)?;
    assert_eq!(fixture.stream.work_stats().full_parses, 1);
    assert_eq!(document.line_count(), 3);
    for index in 0..document.line_count() {
        assert_eq!(document.line(index), update.highlights.line(index));
    }
    fixture.assert_equivalent();
    Ok(())
}

#[test]
fn a_large_document_is_analyzed_in_one_parse() -> TestResult {
    let mut text = String::new();
    for index in 0..10_000 {
        writeln!(text, "fn value_{index}() -> usize {{ {index} }}")?;
    }
    let before = thread_work();
    let analysis = analyze("rust", &text);
    let after = thread_work();
    assert_eq!(after.full_parses - before.full_parses, 1);
    assert_eq!(after.incremental_parses, before.incremental_parses);
    assert!(after.parser_input_bytes - before.parser_input_bytes >= text.len());
    assert!(!line(&analysis.highlights, 9_000)?.is_empty());
    assert_eq!(analysis.symbols.definitions.len(), 10_000);
    Ok(())
}

#[test]
fn highlights_are_theme_independent_and_resolve_per_theme() -> TestResult {
    let source = "<script>const café = 1;</script>";
    let fixture = SyntaxBuilder::default().language("html").build();
    let highlights = fixture.highlight(source);
    let sage = resolved(&highlights, &SyntaxTheme::default());
    let ayu = resolved(&highlights, &ReviewTheme::ayu()?.syntax);
    assert_ne!(sage[0], ayu[0]);
    assert!(sage[0].as_ref().is_some_and(|spans| !spans.is_empty()));
    Ok(())
}

#[test]
fn aliases_resolve_to_identical_utf8_highlights() -> TestResult {
    let source = "let café = 1;\n";
    let alias = SyntaxBuilder::default().language("rs").build().highlight(source);
    let canonical = SyntaxBuilder::default().language("RUST").build().highlight(source);
    assert!(!line(&alias, 0)?.is_empty());
    assert!(
        line(&alias, 0)?
            .iter()
            .all(|span| source.is_char_boundary(span.range.start) && source.is_char_boundary(span.range.end))
    );
    assert_eq!(alias.line(0), canonical.line(0));
    Ok(())
}

#[test]
fn paths_and_shebangs_resolve_before_the_first_parse() -> TestResult {
    for (hint, source) in [
        (LanguageHint::Path("src/lib.rs"), "/* alpha\r\nbeta */\nlet café = 1;\n"),
        (LanguageHint::Auto, "#!/usr/bin/env python3\nprint('hi')\n"),
        (LanguageHint::InfoString("python linenums"), "print('hi')\n"),
    ] {
        let highlights = analyze(hint, source).highlights;
        assert_eq!(highlights.line_count(), source.lines().count());
        for (index, text) in source.lines().enumerate() {
            assert!(!line(&highlights, index)?.is_empty());
            assert!(line(&highlights, index)?.iter().all(|span| span.range.end <= text.len()));
        }
    }
    Ok(())
}

#[test]
fn unknown_language_is_plain_text_without_parsing() -> TestResult {
    let before = thread_work();
    let analysis = analyze("binary.zzz", "abc\n\n");
    assert_eq!(analysis.highlights.line_count(), 2);
    assert!(line(&analysis.highlights, 0)?.is_empty());
    assert!(line(&analysis.highlights, 1)?.is_empty());
    assert_eq!(thread_work().parses(), before.parses());
    assert_eq!(*analysis.symbols, SymbolTags::default());
    Ok(())
}

#[test]
fn input_limits_reject_oversized_sources() -> TestResult {
    let mut fixture = SyntaxBuilder::default().build();
    let accepted = fixture.append("let café=1;")?;
    assert!(matches!(
        fixture.append(&"x".repeat(MAX_SOURCE_BYTES)),
        Err(SyntaxError::InputLimit { limit: MAX_SOURCE_BYTES, attempted }) if attempted == MAX_SOURCE_BYTES + 12
    ));
    assert_eq!(fixture.stream.revision(), accepted.revision);
    let before = thread_work();
    let rejected = analyze("rust", &"x".repeat(MAX_SOURCE_BYTES + 1));
    assert_eq!(thread_work(), before);
    assert_eq!(rejected.highlights.line_count(), 0);
    Ok(())
}

#[test]
fn supported_languages_use_the_same_document_engine() -> TestResult {
    for (language, source) in [
        ("rust", "fn main() {}"),
        ("js", "const x = true;"),
        ("jsx", "const view = <Panel title=\"Hi\" />;"),
        ("typescript", "const x: number = 1;"),
        ("tsx", "const view = <Panel />;"),
        ("py", "def f(): return 1"),
        ("sh", "echo hi"),
        ("c", "int main(void) {}"),
        ("cpp", "class C {};"),
        ("go", "package main"),
        ("json", "{\"x\": true}"),
        ("jsonc", "{\"x\": true /* comment */}"),
        ("toml", "x = 1"),
        ("just", "greet name:\n\techo {{ name }}\n"),
        ("yml", "x: true"),
        ("html", "<b>x</b>"),
        ("css", "b { color: red; }"),
        ("md", "# Heading"),
    ] {
        let mut fixture = SyntaxBuilder::default().language(language).build();
        fixture.append(source)?;
        assert!(!line(fixture.stream.highlights(), 0)?.is_empty(), "{language}");
        fixture.assert_equivalent();
    }
    Ok(())
}

#[test]
fn injections_highlight_embedded_languages() -> TestResult {
    for (language, source, index) in [
        ("html", "<p>café</p><script>const π = 3.14;</script><style>b{color:red}</style>", 0),
        ("markdown", "# Title\n\n```rust\nfn main() {}\n```\n", 3),
    ] {
        let mut fixture = SyntaxBuilder::default().language(language).build();
        fixture.append(source)?;
        assert!(!line(fixture.stream.highlights(), index)?.is_empty());
        fixture.assert_equivalent();
    }
    Ok(())
}

#[test]
fn clipped_lines_keep_only_spans_that_end_within_the_length() -> TestResult {
    let highlights = analyze("rust", "fn main() { let value = 1; }").highlights;
    let full = line(&highlights, 0)?;
    let ends: Vec<usize> = full.iter().map(|span| span.range.end).collect();
    let cut = *ends.get(1).ok_or("two spans")?;
    let clipped = full.clipped(cut + 1);
    assert_eq!(clipped.len(), ends.iter().filter(|&&end| end <= cut + 1).count());
    assert!(clipped.iter().all(|span| span.range.end <= cut + 1));
    assert_eq!(full.clipped(usize::MAX), full);
    assert!(full.clipped(0).is_empty());
    Ok(())
}

#[derive(Clone)]
struct Rust;

impl Language for Rust {
    fn kind_to_id(&self, kind: &str) -> u16 {
        self.get_ts_language().id_for_node_kind(kind, true)
    }

    fn field_to_id(&self, field: &str) -> Option<u16> {
        self.get_ts_language().field_id_for_name(field).map(NonZeroU16::get)
    }

    fn build_pattern(&self, builder: &PatternBuilder) -> Result<Pattern, PatternError> {
        builder.build(|source| StrDoc::try_new(source, self.clone()))
    }
}

impl LanguageExt for Rust {
    fn get_ts_language(&self) -> TSLanguage {
        arborium_rust::language().into()
    }
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
