use clankerdiff_syntax::{
    CacheConfig, DocumentHighlights, HighlightSpan, HighlightStats, LanguageHint, SourceSequenceId,
    SyntaxHighlighter, SyntaxStream, SyntaxStreamError, SyntaxTheme,
};
use clankerdiff_theme::ReviewTheme;
use std::{error::Error, fmt::Write, sync::Arc};

#[test]
fn hits_promote_source_and_document_entries() {
    for documents in [false, true] {
        let mut fixture = CacheBuilder::default().entries(2).build(documents);
        fixture.highlight("let a = 1;");
        fixture.highlight("let b = 2;");
        fixture.highlight("let a = 1;");
        fixture.highlight("let c = 3;");
        assert_eq!(fixture.highlighter.stats().evictions, 1);
        fixture.highlighter.reset_stats();
        fixture.highlight("let a = 1;");
        fixture.highlight("let b = 2;");
        assert_eq!(fixture.highlighter.stats().hits, 1);
        assert_eq!(fixture.highlighter.stats().misses, 1);
    }
}

#[test]
fn entry_limits_and_clear_apply_to_both_caches() {
    for documents in [false, true] {
        let mut fixture = CacheBuilder::default().entries(2).build(documents);
        for source in ["let a = 1;", "let b = 2;", "let c = 3;", "let d = 4;"] {
            fixture.highlight(source);
        }
        assert_eq!(fixture.highlighter.stats().evictions, 2);
        let usage = fixture.highlighter.cache_usage();
        assert_eq!(usage.span_entries, if documents { 0 } else { 2 });
        assert_eq!(usage.document_entries, if documents { 2 } else { 0 });
        fixture.highlighter.clear_cache();
        assert_eq!(fixture.highlighter.cache_usage(), Default::default());
        fixture.highlighter.reset_stats();
        fixture.highlight("let d = 4;");
        assert_eq!(fixture.highlighter.stats().misses, 1);
        assert_eq!(fixture.highlighter.stats().evictions, 0);
    }
}

#[test]
fn zero_capacity_disables_both_caches() {
    for documents in [false, true] {
        let mut fixture = CacheBuilder::default().entries(0).build(documents);
        fixture.highlight("let a = 1;");
        fixture.highlight("let a = 1;");
        assert_eq!(fixture.highlighter.stats().misses, 2);
        assert_eq!(fixture.highlighter.stats().evictions, 0);
        assert_eq!(fixture.highlighter.cache_usage(), Default::default());
    }
}

#[test]
fn stream_append_projects_every_line_including_multibyte_text() -> Result<(), Box<dyn Error>> {
    let mut highlighter = SyntaxHighlighter::default();
    let theme = SyntaxTheme::default();
    let mut stream = SyntaxStream::new("rust");
    let update = highlighter
        .with_theme(&theme)
        .append(&mut stream, ["fn main() {", "  println!(\"é\");", "}"])?;
    assert_eq!(update.highlights.line_count(), 3);
    assert_eq!(stream.source(), "fn main() {\n  println!(\"é\");\n}\n");
    assert!(!line(&update.highlights, 1)?.is_empty());
    let spans = highlighter
        .with_theme(&theme)
        .highlight_source(LanguageHint::Id("rust"), "let x = 1;");
    assert!(!spans.is_empty());
    Ok(())
}

#[test]
fn document_highlights_and_take_stats_are_public_contracts() -> Result<(), Box<dyn Error>> {
    let text = "/* open\nclosed */\n";
    let id = SourceSequenceId::from_lines(text.lines());
    let mut highlighter = SyntaxHighlighter::default();
    let theme = SyntaxTheme::default();
    let highlights = highlighter
        .with_theme(&theme)
        .highlight_document(id, "rust", text);
    assert_eq!(highlights.line_count(), 2);
    assert!(!line(&highlights, 1)?.is_empty());
    let first = highlighter.take_stats();
    assert_eq!(first.calls, 1);
    assert_eq!(first.misses, 1);
    assert_eq!(first.bytes, text.len());
    assert_eq!(highlighter.take_stats(), HighlightStats::default());
    Ok(())
}

#[test]
fn complete_document_is_parsed_once_then_lines_are_constant_time_lookups()
-> Result<(), Box<dyn Error>> {
    let mut text = String::new();
    for index in 0..10_000 {
        writeln!(text, "let value_{index} = {index};")?;
    }
    let id = SourceSequenceId::from_lines(text.lines());
    let theme = SyntaxTheme::default();
    let config = CacheConfig {
        max_entries: 64,
        max_documents: 4,
        ..CacheConfig::default()
    };
    let mut highlighter = SyntaxHighlighter::new(config);
    let highlights = highlighter
        .with_theme(&theme)
        .highlight_document(id, "rust", &text);
    assert!(!line(&highlights, 9_000)?.is_empty());
    let first_parse = highlighter.take_stats();
    assert_eq!(first_parse.calls, 1);
    assert_eq!(first_parse.misses, 1);
    assert_eq!(first_parse.bytes, text.len());
    for index in [0, 5_000, 9_999] {
        assert!(highlights.line(index).is_some());
    }
    assert_eq!(highlighter.stats(), HighlightStats::default());
    let cached = highlighter
        .with_theme(&theme)
        .highlight_document(id, "rust", &text);
    assert!(Arc::ptr_eq(&highlights, &cached));
    assert_eq!(highlighter.stats().calls, 1);
    assert_eq!(highlighter.stats().hits, 1);
    assert_eq!(highlighter.stats().bytes, 0);
    Ok(())
}

#[test]
fn line_sequences_detect_a_shebang_on_the_first_line() -> Result<(), Box<dyn Error>> {
    let theme = SyntaxTheme::default();
    let mut highlighter = SyntaxHighlighter::default();
    let lines = ["#!/usr/bin/env python3", "print('hi')"];
    let highlights = highlighter.with_theme(&theme).highlight_document_lines(
        SourceSequenceId::from_lines(lines),
        LanguageHint::Auto,
        lines,
    );
    assert!(!line(&highlights, 1)?.is_empty());
    Ok(())
}

#[test]
fn document_cache_evictions_are_counted() {
    let theme = SyntaxTheme::default();
    let mut highlighter = SyntaxHighlighter::new(CacheConfig {
        max_documents: 1,
        ..CacheConfig::default()
    });
    for text in ["let a = 1;\n", "let b = 2;\n"] {
        let id = SourceSequenceId::from_lines(text.lines());
        highlighter
            .with_theme(&theme)
            .highlight_document(id, "rust", text);
    }
    assert_eq!(highlighter.take_stats().evictions, 1);
}

#[test]
fn stream_appends_do_not_evict_cached_documents() -> Result<(), Box<dyn Error>> {
    let theme = SyntaxTheme::default();
    let mut highlighter = SyntaxHighlighter::new(CacheConfig {
        max_documents: 1,
        ..CacheConfig::default()
    });
    let text = "fn main() {}\n";
    let id = SourceSequenceId::from_lines(text.lines());
    highlighter
        .with_theme(&theme)
        .highlight_document(id, "rust", text);
    let mut stream = SyntaxStream::new("rust");
    for index in 0..4 {
        let appended = format!("let value_{index} = {index};");
        highlighter
            .with_theme(&theme)
            .append(&mut stream, [appended.as_str()])?;
    }
    highlighter.take_stats();
    highlighter
        .with_theme(&theme)
        .highlight_document(id, "rust", text);
    assert_eq!(
        highlighter.take_stats(),
        HighlightStats {
            calls: 1,
            hits: 1,
            ..HighlightStats::default()
        }
    );
    Ok(())
}

#[test]
fn stream_highlights_long_multiline_constructs_with_complete_context() -> Result<(), Box<dyn Error>>
{
    let theme = SyntaxTheme::default();
    let mut highlighter = SyntaxHighlighter::default();
    let mut stream = SyntaxStream::new("rust");
    let mut lines = vec!["/* opening comment"];
    lines.extend(std::iter::repeat_n("still comment", 1_100));
    highlighter
        .with_theme(&theme)
        .append(&mut stream, lines.iter().copied())?;
    let actual = highlighter
        .with_theme(&theme)
        .append(&mut stream, ["let still_comment = 1;"])?;
    lines.push("let still_comment = 1;");
    let expected = highlighter
        .with_theme(&theme)
        .highlight_lines("rust", lines.iter().copied());
    assert_eq!(
        actual.highlights.line(lines.len() - 1),
        expected.last().map(Vec::as_slice)
    );
    Ok(())
}

#[test]
fn cloned_streams_append_independently() -> Result<(), Box<dyn Error>> {
    let theme = SyntaxTheme::default();
    let mut highlighter = SyntaxHighlighter::default();
    let mut stable = SyntaxStream::new("rust");
    highlighter
        .with_theme(&theme)
        .append(&mut stable, ["/* stable"])?;
    let mut speculative = stable.clone();
    highlighter
        .with_theme(&theme)
        .append(&mut speculative, ["closed */"])?;
    let update = highlighter
        .with_theme(&theme)
        .append(&mut stable, ["still comment"])?;
    let expected = highlighter
        .with_theme(&theme)
        .highlight_lines("rust", ["/* stable", "still comment"]);
    assert_eq!(
        update.highlights.line(1),
        expected.last().map(Vec::as_slice)
    );
    assert!(!stable.source().contains("closed"));
    Ok(())
}

#[test]
fn stream_updates_report_restyled_earlier_lines_and_reuse_unchanged_projections()
-> Result<(), Box<dyn Error>> {
    let theme = SyntaxTheme::default();
    let mut highlighter = SyntaxHighlighter::default();
    let mut stream = SyntaxStream::new("rust");
    let first = highlighter
        .with_theme(&theme)
        .append(&mut stream, ["let value = r#\"open"])?;
    let update = highlighter
        .with_theme(&theme)
        .append(&mut stream, ["closed\"#;"])?;
    let expected = highlighter
        .with_theme(&theme)
        .highlight_lines("rust", stream.source().lines());
    for (index, spans) in expected.iter().enumerate() {
        assert_eq!(update.highlights.line(index), Some(spans.as_slice()));
        if first.highlights.line(index) != Some(spans.as_slice()) {
            assert!(update.changed_lines.contains(&index));
        }
    }
    highlighter.clear_cache();
    highlighter.take_stats();
    let unchanged = highlighter.with_theme(&theme).append(&mut stream, [])?;
    assert!(unchanged.changed_lines.is_empty());
    assert!(Arc::ptr_eq(&unchanged.highlights, &update.highlights));
    assert_eq!(unchanged.base_revision, unchanged.revision);
    assert_eq!(highlighter.take_stats(), HighlightStats::default());
    let changed_theme = ReviewTheme::ayu()?.syntax;
    let recolored = highlighter
        .with_theme(&changed_theme)
        .append(&mut stream, [])?;
    let expected = highlighter
        .with_theme(&changed_theme)
        .highlight_lines("rust", stream.source().lines());
    assert_eq!(recolored.revision, unchanged.revision);
    assert!(!recolored.changed_lines.is_empty());
    for (index, spans) in expected.iter().enumerate() {
        assert_eq!(recolored.highlights.line(index), Some(spans.as_slice()));
    }
    Ok(())
}

#[test]
fn oversized_updates_are_atomic_including_a_single_long_line() -> Result<(), Box<dyn Error>> {
    let theme = SyntaxTheme::default();
    let mut highlighter = SyntaxHighlighter::new(CacheConfig {
        max_stream_bytes: 16,
        ..CacheConfig::default()
    });
    let mut stream = SyntaxStream::new("rust");
    let accepted = highlighter
        .with_theme(&theme)
        .append(&mut stream, ["/* open"])?;
    let source = stream.source().to_owned();
    highlighter.take_stats();
    for lines in [
        vec!["a", "this line exceeds the budget"],
        vec!["0123456789abcdef"],
    ] {
        let result = highlighter.with_theme(&theme).append(&mut stream, lines);
        assert!(matches!(
            result,
            Err(SyntaxStreamError::InputLimit { limit: 16, .. })
        ));
        assert_eq!(stream.source(), source);
        assert_eq!(stream.revision(), accepted.revision);
        assert!(Arc::ptr_eq(stream.highlights(), &accepted.highlights));
        assert_eq!(highlighter.take_stats(), HighlightStats::default());
    }
    let next = highlighter
        .with_theme(&theme)
        .append(&mut stream, ["end */"])?;
    assert_eq!(next.revision, accepted.revision + 1);
    Ok(())
}

#[test]
fn aliases_and_utf8_ranges_share_a_cache_entry() {
    let theme = SyntaxTheme::default();
    let mut highlighter = SyntaxHighlighter::new(2);
    let source = "let café = 1;\n";
    let spans = highlighter
        .with_theme(&theme)
        .highlight_source("rs", source);
    assert!(!spans.is_empty());
    assert!(
        spans
            .iter()
            .all(|span| source.is_char_boundary(span.range.start)
                && source.is_char_boundary(span.range.end))
    );
    highlighter
        .with_theme(&theme)
        .highlight_source("RUST", source);
    assert_eq!(highlighter.stats().hits, 1);
}

#[test]
fn complete_documents_are_parsed_once_and_projected_by_line() -> Result<(), Box<dyn Error>> {
    let theme = SyntaxTheme::default();
    let mut highlighter = SyntaxHighlighter::default();
    let source = "/* alpha\r\nbeta */\nlet café = 1;\n";
    let sequence = SourceSequenceId::from_lines(source.split_terminator('\n'));

    let first = highlighter.with_theme(&theme).highlight_document(
        sequence,
        LanguageHint::Path("src/lib.rs"),
        source,
    );
    assert_eq!(first.line_count(), 3);
    assert!(!line(&first, 0)?.is_empty());
    assert!(!line(&first, 1)?.is_empty());
    assert!(
        line(&first, 2)?
            .iter()
            .all(|span| span.range.end <= "let café = 1;".len())
    );
    let parsed_bytes = highlighter.stats().bytes;

    let second = highlighter.with_theme(&theme).highlight_document(
        sequence,
        LanguageHint::Path("src/lib.rs"),
        source,
    );
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(highlighter.stats().hits, 1);
    assert_eq!(highlighter.stats().bytes, parsed_bytes);

    let empty = highlighter.with_theme(&theme).highlight_document(
        SourceSequenceId::from_lines([]),
        "rust",
        "",
    );
    assert_eq!(empty.line_count(), 0);
    Ok(())
}

#[test]
fn line_sequences_parse_once_and_keep_multiline_context() -> Result<(), Box<dyn Error>> {
    let theme = SyntaxTheme::default();
    let mut highlighter = SyntaxHighlighter::default();
    let lines = ["/* alpha", "beta", "gamma */", "let x = 1;"];
    let sequence = SourceSequenceId::from_lines(lines);

    let first = highlighter.with_theme(&theme).highlight_document_lines(
        sequence,
        LanguageHint::Path("src/lib.rs"),
        lines,
    );
    assert_eq!(first.line_count(), 4);
    for (index, source) in lines.iter().enumerate() {
        let spans = line(&first, index)?;
        assert!(!spans.is_empty(), "line {index}");
        assert!(spans.iter().all(|span| span.range.end <= source.len()));
    }

    let second = highlighter.with_theme(&theme).highlight_document_lines(
        sequence,
        LanguageHint::Path("src/lib.rs"),
        lines,
    );
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(highlighter.stats().misses, 1);
    assert_eq!(highlighter.stats().hits, 1);
    Ok(())
}

#[test]
fn supported_language_bundle_highlights_representative_sources() {
    let cases = [
        ("rust", "fn main() {}"),
        ("js", "const x = true;"),
        ("jsx", "const view = <Panel title=\"Hi\" />;"),
        ("typescript", "const x: number = 1;"),
        ("tsx", "const view = <Panel title=\"Hi\" />;"),
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
    ];
    let theme = SyntaxTheme::default();
    let mut highlighter = SyntaxHighlighter::new(cases.len());
    for (language, source) in cases {
        let spans = highlighter
            .with_theme(&theme)
            .highlight_source(language, source);
        assert!(!spans.is_empty(), "{language}");
        assert_valid_spans(source, &spans);
    }
}

#[test]
fn unknown_language_is_plain_text_and_cached() {
    let mut highlighter = SyntaxHighlighter::new(2);
    let theme = SyntaxTheme::default();
    assert!(
        highlighter
            .with_theme(&theme)
            .highlight_source("binary.zzz", "abc")
            .is_empty()
    );
    assert!(
        highlighter
            .with_theme(&theme)
            .highlight_source("binary.zzz", "abc")
            .is_empty()
    );
    assert_eq!(highlighter.stats().hits, 1);
    assert_eq!(highlighter.stats().bytes, 0);
}

#[test]
fn eviction_zero_capacity_and_theme_revision_behave_as_before() -> Result<(), Box<dyn Error>> {
    let theme = SyntaxTheme::default();
    let mut cache = SyntaxHighlighter::new(1);
    cache
        .with_theme(&theme)
        .highlight_source("rust", "fn a() {}");
    cache
        .with_theme(&theme)
        .highlight_source("rust", "fn b() {}");
    assert_eq!(cache.stats().evictions, 1);
    let mut zero = SyntaxHighlighter::new(0);
    zero.with_theme(&theme)
        .highlight_source("rust", "fn a() {}");
    zero.with_theme(&theme)
        .highlight_source("rust", "fn a() {}");
    assert_eq!(zero.stats().misses, 2);
    let mut themes = SyntaxHighlighter::new(8);
    themes
        .with_theme(&theme)
        .highlight_source("rust", "fn main() {}");
    themes
        .with_theme(&ReviewTheme::ayu()?.syntax)
        .highlight_source("rust", "fn main() {}");
    assert_eq!(themes.stats().misses, 2);
    Ok(())
}

#[test]
fn multiline_and_synthetic_newlines_are_clipped() {
    let theme = SyntaxTheme::default();
    let mut highlighter = SyntaxHighlighter::new(0);
    let lines = ["/*", " café */"];
    let rendered = highlighter
        .with_theme(&theme)
        .highlight_lines("rust", lines);
    assert_eq!(rendered.len(), 2);
    assert!(rendered.iter().all(|spans| !spans.is_empty()));
    for (source, spans) in lines.into_iter().zip(rendered) {
        assert!(spans.iter().all(|span| span.range.end <= source.len()
            && source.is_char_boundary(span.range.start)
            && source.is_char_boundary(span.range.end)));
    }
}

#[test]
fn html_and_markdown_injections_highlight_embedded_languages() -> Result<(), Box<dyn Error>> {
    let theme = SyntaxTheme::default();
    let mut highlighter = SyntaxHighlighter::new(4);
    let html = "<p>café</p><script>const π = 3.14;</script><style>b{color:red}</style>";
    let html_spans = highlighter
        .with_theme(&theme)
        .highlight_source("html", html);
    assert_valid_spans(html, &html_spans);
    for needle in ["const π", "color:red"] {
        let range = text_range(html, needle)?;
        assert!(
            html_spans
                .iter()
                .any(|span| span.range.start < range.end && span.range.end > range.start),
            "no injected highlight for {needle}"
        );
    }

    let markdown = "# Title\n\n```rust\nfn main() {}\n```\n";
    let markdown_spans = highlighter
        .with_theme(&theme)
        .highlight_source("markdown", markdown);
    assert_valid_spans(markdown, &markdown_spans);
    let range = text_range(markdown, "fn main")?;
    assert!(
        markdown_spans
            .iter()
            .any(|span| span.range.start < range.end && span.range.end > range.start),
        "no injected Rust highlight in Markdown"
    );
    Ok(())
}

#[derive(Default)]
struct CacheBuilder {
    config: CacheConfig,
}

impl CacheBuilder {
    fn entries(mut self, limit: usize) -> Self {
        self.config.max_entries = limit;
        self.config.max_documents = limit;
        self
    }

    fn build(self, documents: bool) -> CacheFixture {
        CacheFixture {
            highlighter: SyntaxHighlighter::new(self.config),
            theme: SyntaxTheme::default(),
            documents,
        }
    }
}

fn line(highlights: &DocumentHighlights, index: usize) -> Result<&[HighlightSpan], Box<dyn Error>> {
    highlights
        .line(index)
        .ok_or_else(|| format!("missing line {index}").into())
}

fn assert_valid_spans(source: &str, spans: &[HighlightSpan]) {
    let mut previous_end = 0;
    for span in spans {
        assert!(span.range.start < span.range.end, "empty span: {span:?}");
        assert!(
            span.range.end <= source.len(),
            "out-of-bounds span: {span:?}"
        );
        assert!(source.is_char_boundary(span.range.start));
        assert!(source.is_char_boundary(span.range.end));
        assert!(
            span.range.start >= previous_end,
            "overlapping or unordered span: {span:?}"
        );
        previous_end = span.range.end;
    }
}

fn text_range(source: &str, needle: &str) -> Result<std::ops::Range<usize>, Box<dyn Error>> {
    let start = source
        .find(needle)
        .ok_or_else(|| format!("missing {needle:?}"))?;
    Ok(start..start + needle.len())
}

struct CacheFixture {
    highlighter: SyntaxHighlighter,
    theme: SyntaxTheme,
    documents: bool,
}

impl CacheFixture {
    fn highlight(&mut self, source: &str) {
        let mut themed = self.highlighter.with_theme(&self.theme);
        if self.documents {
            themed.highlight_document(SourceSequenceId::from_lines([source]), "rust", source);
        } else {
            themed.highlight_source("rust", source);
        }
    }
}
