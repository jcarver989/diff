mod support;

use ast_grep_core::{
    AstGrep, Language, Pattern, PatternError,
    matcher::PatternBuilder,
    tree_sitter::{LanguageExt, StrDoc, TSLanguage},
};
use clankerdiff_syntax::{CacheUsage, Fingerprint, HighlightStats, LanguageHint, SyntaxError};
use clankerdiff_theme::ReviewTheme;
use std::{fmt::Write, num::NonZeroU16, sync::Arc};
use support::{CacheBuilder, TestResult, line};

#[test]
fn shares_upstream_runtime_with_ast_grep() -> TestResult {
    let source = "fn main() {}";
    let mut fixture = CacheBuilder::default().build();
    let highlights = fixture.highlight(source)?;
    assert!(!line(&highlights, 0)?.is_empty());
    let document = StrDoc::try_new(source, Rust)?;
    let ast = AstGrep::doc(document);
    let pattern = Pattern::try_new(source, Rust)?;
    let root = ast.root();
    let found = root
        .find(&pattern)
        .ok_or("ast-grep did not match Rust function")?;
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
        let mut fixture = CacheBuilder::default().language(language).build();
        fixture.append(source)?;
        fixture.assert_equivalent()?;
        let highlights = fixture.stream.highlights();
        let lines: Vec<_> = (0..highlights.line_count())
            .map(|index| highlights.line(index))
            .collect();
        assert_eq!(
            Fingerprint::of([format!("{lines:?}")]).to_string(),
            expected,
            "{language}: {source:?}: {lines:?}"
        );
    }
    Ok(())
}

#[test]
fn hits_promote_document_entries() -> TestResult {
    let mut fixture = CacheBuilder::default().entries(2).build();
    for source in ["let a = 1;", "let b = 2;", "let a = 1;", "let c = 3;"] {
        fixture.highlight(source)?;
    }
    assert_eq!(fixture.highlighter.stats().evictions, 1);
    fixture.highlighter.reset_stats();
    fixture.highlight("let a = 1;")?;
    fixture.highlight("let b = 2;")?;
    assert_eq!(fixture.highlighter.stats().hits, 1);
    assert_eq!(fixture.highlighter.stats().misses, 1);
    Ok(())
}

#[test]
fn entry_limits_and_clear_apply_to_retained_documents() -> TestResult {
    let mut fixture = CacheBuilder::default().entries(2).build();
    for source in ["let a = 1;", "let b = 2;", "let c = 3;", "let d = 4;"] {
        fixture.highlight(source)?;
    }
    assert_eq!(fixture.highlighter.stats().evictions, 2);
    assert_eq!(fixture.highlighter.cache_usage().document_entries, 2);
    fixture.highlighter.clear_cache();
    assert_eq!(fixture.highlighter.cache_usage(), CacheUsage::default());
    fixture.highlighter.reset_stats();
    fixture.highlight("let d = 4;")?;
    assert_eq!(fixture.highlighter.stats().misses, 1);
    assert_eq!(fixture.highlighter.stats().evictions, 0);
    Ok(())
}

#[test]
fn zero_capacity_disables_document_caching() -> TestResult {
    let mut fixture = CacheBuilder::default().entries(0).build();
    fixture.highlight("let a = 1;")?;
    fixture.highlight("let a = 1;")?;
    assert_eq!(fixture.highlighter.stats().misses, 2);
    assert_eq!(fixture.highlighter.stats().evictions, 0);
    assert_eq!(fixture.highlighter.cache_usage(), CacheUsage::default());
    Ok(())
}

#[test]
fn complete_input_and_first_append_perform_identical_work() -> TestResult {
    let mut fixture = CacheBuilder::default().build();
    let source = "/* open\nclosed */\nlet café = 1;\n";
    let document = fixture.highlight(source)?;
    let complete = fixture.highlighter.take_stats();
    assert_eq!(complete.calls, 1);
    assert_eq!(complete.misses, 1);
    assert!(complete.bytes >= source.len());
    let update = fixture.append(source)?;
    assert_eq!(fixture.highlighter.take_stats().bytes, complete.bytes);
    assert_eq!(fixture.stream.work_stats().full_parses, 1);
    assert_eq!(document.line_count(), 3);
    for index in 0..document.line_count() {
        assert_eq!(document.line(index), update.highlights.line(index));
    }
    fixture.assert_equivalent()?;
    assert_eq!(fixture.highlighter.take_stats(), HighlightStats::default());
    Ok(())
}

#[test]
fn complete_document_is_parsed_once_then_reused_without_loading_source() -> TestResult {
    let mut text = String::new();
    for index in 0..10_000 {
        writeln!(text, "let value_{index} = {index};")?;
    }
    let mut fixture = CacheBuilder::default().entries(4).build();
    let highlights = fixture.highlight(&text)?;
    assert!(!line(&highlights, 9_000)?.is_empty());
    let first = fixture.highlighter.take_stats();
    assert_eq!(first.calls, 1);
    assert_eq!(first.misses, 1);
    assert!(first.bytes >= text.len());
    for index in [0, 5_000, 9_999] {
        assert!(highlights.line(index).is_some());
    }
    assert_eq!(fixture.highlighter.stats(), HighlightStats::default());
    let cached = fixture
        .highlighter
        .with_theme(&fixture.theme)
        .highlight_document(Fingerprint::of([text.as_str()]), "rust", || -> &str {
            panic!("cached source must not be loaded")
        })?;
    assert!(Arc::ptr_eq(&highlights, &cached));
    assert_eq!(
        fixture.highlighter.stats(),
        HighlightStats {
            calls: 1,
            hits: 1,
            ..HighlightStats::default()
        }
    );
    Ok(())
}

#[test]
fn cached_documents_recolor_without_parsing_or_loading_source() -> TestResult {
    let mut fixture = CacheBuilder::default().entries(1).build();
    let source = "<script>const café = 1;</script>";
    let mut html = CacheBuilder::default().language("html").build();
    let first = html.highlight(source)?;
    html.highlighter.take_stats();
    html.theme = ReviewTheme::ayu()?.syntax;
    let recolored = html
        .highlighter
        .with_theme(&html.theme)
        .highlight_document(Fingerprint::of([source]), "html", || -> &str {
            panic!("recoloring must use retained source")
        })?;
    assert_ne!(first.line(0), recolored.line(0));
    assert_eq!(
        html.highlighter.stats(),
        HighlightStats {
            calls: 1,
            hits: 1,
            ..HighlightStats::default()
        }
    );
    html.append(source)?;
    assert_eq!(recolored.line(0), html.stream.highlights().line(0));
    html.assert_equivalent()?;
    fixture.highlight("fn first() {}")?;
    fixture.theme = ReviewTheme::ayu()?.syntax;
    fixture.highlight("fn first() {}")?;
    assert_eq!(fixture.highlighter.cache_usage().document_entries, 1);
    assert_eq!(fixture.highlighter.stats().evictions, 0);
    Ok(())
}

#[test]
fn stream_updates_do_not_evict_cached_documents() -> TestResult {
    let mut fixture = CacheBuilder::default().entries(1).build();
    let text = "fn main() {}\n";
    fixture.highlight(text)?;
    for index in 0..4 {
        fixture.append(&format!("let value_{index} = {index};\n"))?;
    }
    fixture.highlighter.take_stats();
    fixture.highlight(text)?;
    assert_eq!(
        fixture.highlighter.take_stats(),
        HighlightStats {
            calls: 1,
            hits: 1,
            ..HighlightStats::default()
        }
    );
    Ok(())
}

#[test]
fn aliases_and_utf8_ranges_share_a_cache_entry() -> TestResult {
    let mut fixture = CacheBuilder::default().language("rs").build();
    let source = "let café = 1;\n";
    let highlights = fixture.highlight(source)?;
    assert!(!line(&highlights, 0)?.is_empty());
    assert!(
        line(&highlights, 0)?
            .iter()
            .all(|span| source.is_char_boundary(span.range.start)
                && source.is_char_boundary(span.range.end))
    );
    let cached = fixture
        .highlighter
        .with_theme(&fixture.theme)
        .highlight_document(Fingerprint::of([source]), "RUST", || source)?;
    assert!(Arc::ptr_eq(&highlights, &cached));
    assert_eq!(fixture.highlighter.stats().hits, 1);
    Ok(())
}

#[test]
fn paths_and_shebangs_resolve_before_the_first_parse() -> TestResult {
    let mut fixture = CacheBuilder::default().build();
    for (hint, source) in [
        (
            LanguageHint::Path("src/lib.rs"),
            "/* alpha\r\nbeta */\nlet café = 1;\n",
        ),
        (LanguageHint::Auto, "#!/usr/bin/env python3\nprint('hi')\n"),
        (LanguageHint::InfoString("python linenums"), "print('hi')\n"),
    ] {
        let highlights = fixture
            .highlighter
            .with_theme(&fixture.theme)
            .highlight_document(Fingerprint::of([source]), hint, || source)?;
        assert_eq!(highlights.line_count(), source.lines().count());
        for (index, text) in source.lines().enumerate() {
            assert!(!line(&highlights, index)?.is_empty());
            assert!(
                line(&highlights, index)?
                    .iter()
                    .all(|span| span.range.end <= text.len())
            );
        }
    }
    Ok(())
}

#[test]
fn unknown_language_is_plain_text_and_cached() -> TestResult {
    let mut fixture = CacheBuilder::default().language("binary.zzz").build();
    for _ in 0..2 {
        let highlights = fixture.highlight("abc\n\n")?;
        assert_eq!(highlights.line_count(), 2);
        assert!(line(&highlights, 0)?.is_empty());
        assert!(line(&highlights, 1)?.is_empty());
    }
    assert_eq!(fixture.highlighter.stats().hits, 1);
    assert_eq!(fixture.highlighter.stats().bytes, 0);
    Ok(())
}

#[test]
fn complete_input_limits_leave_cache_and_counters_unchanged() -> TestResult {
    let mut fixture = CacheBuilder::default().source_limit(12).build();
    let accepted = fixture.highlight("let café=1;")?;
    let stats = fixture.highlighter.stats();
    let usage = fixture.highlighter.cache_usage();
    assert!(matches!(
        fixture.highlight("let café = 123;"),
        Err(SyntaxError::InputLimit { limit: 12, .. })
    ));
    assert_eq!(fixture.highlighter.stats(), stats);
    assert_eq!(fixture.highlighter.cache_usage(), usage);
    assert!(Arc::ptr_eq(&accepted, &fixture.highlight("let café=1;")?));
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
        let mut fixture = CacheBuilder::default().language(language).build();
        fixture.append(source)?;
        assert!(
            !line(fixture.stream.highlights(), 0)?.is_empty(),
            "{language}"
        );
        fixture.assert_equivalent()?;
    }
    Ok(())
}

#[test]
fn injections_highlight_embedded_languages() -> TestResult {
    for (language, source, index) in [
        (
            "html",
            "<p>café</p><script>const π = 3.14;</script><style>b{color:red}</style>",
            0,
        ),
        ("markdown", "# Title\n\n```rust\nfn main() {}\n```\n", 3),
    ] {
        let mut fixture = CacheBuilder::default().language(language).build();
        fixture.append(source)?;
        assert!(!line(fixture.stream.highlights(), index)?.is_empty());
        fixture.assert_equivalent()?;
    }
    Ok(())
}

#[derive(Clone)]
struct Rust;

impl Language for Rust {
    fn kind_to_id(&self, kind: &str) -> u16 {
        self.get_ts_language().id_for_node_kind(kind, true)
    }

    fn field_to_id(&self, field: &str) -> Option<u16> {
        self.get_ts_language()
            .field_id_for_name(field)
            .map(NonZeroU16::get)
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
