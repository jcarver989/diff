mod support;

use clankerdiff_syntax::{HighlightStats, SyntaxError};
use clankerdiff_theme::ReviewTheme;
use std::{fmt::Write, str, sync::Arc};
use support::{CacheBuilder, TestResult, line};

#[test]
fn growing_code_and_plain_text_have_linear_work() -> TestResult {
    for language in ["rust", "binary.zzz", ""] {
        let mut fixture = CacheBuilder::default()
            .language(language)
            .entries(0)
            .build();
        let chunk = "let value = 123;\n".repeat(16);
        for _ in 0..96 {
            fixture.append(&chunk)?;
        }
        let budget = 4 * fixture.stream.source().len() + 64 * 1024;
        let work = fixture.stream.work_stats();
        assert!(work.parser_input_bytes <= budget, "{language}: {work:?}");
        assert!(work.queried_bytes <= budget, "{language}: {work:?}");
        assert!(work.projected_bytes <= budget, "{language}: {work:?}");
        assert_eq!(fixture.highlighter.stats().bytes, work.parser_input_bytes);
        assert_eq!(work.full_parses, usize::from(language == "rust"));
        assert_eq!(
            work.incremental_parses,
            if language == "rust" { 95 } else { 0 }
        );
        fixture.assert_equivalent()?;
    }
    Ok(())
}

#[test]
fn aether_code_fixture_meets_the_parser_and_query_budget() -> TestResult {
    let mut source = String::new();
    let mut line = 0;
    while source.len() < 24 * 1024 {
        writeln!(
            source,
            "let value_{line} = state.reconcile(incoming[{line}]).expect(\"delta accepted\");"
        )?;
        line += 1;
    }
    let mut fixture = CacheBuilder::default().build();
    for chunk in source.as_bytes().chunks(256) {
        fixture.append(str::from_utf8(chunk)?)?;
    }
    let work = fixture.stream.work_stats();
    let budget = 4 * source.len() + 64 * 1024;
    assert!(
        work.parser_input_bytes <= budget,
        "{work:?}, budget {budget}"
    );
    assert!(work.queried_bytes <= budget, "{work:?}, budget {budget}");
    assert!(work.projected_bytes <= budget, "{work:?}, budget {budget}");
    fixture.assert_equivalent()
}

#[test]
fn aether_open_function_fixture_meets_the_parser_and_query_budget() -> TestResult {
    let mut fixture = CacheBuilder::default().build();
    fixture.append("fn main() {\n")?;
    let mut source = String::new();
    let mut line = 0;
    while source.len() < 24 * 1024 {
        writeln!(
            source,
            "let value_{line} = state.reconcile(incoming[{line}]).expect(\"delta accepted\");"
        )?;
        line += 1;
    }
    for chunk in source.as_bytes().chunks(256) {
        fixture.append(str::from_utf8(chunk)?)?;
    }
    let work = fixture.stream.work_stats();
    let budget = 4 * fixture.stream.source().len() + 64 * 1024;
    assert!(
        work.parser_input_bytes <= budget,
        "{work:?}, budget {budget}"
    );
    assert!(work.queried_bytes <= budget, "{work:?}, budget {budget}");
    assert!(work.projected_bytes <= budget, "{work:?}, budget {budget}");
    assert!(work.compared_nodes <= budget, "{work:?}, budget {budget}");
    fixture.assert_equivalent()?;
    fixture.append("}\n")?;
    fixture.assert_equivalent()
}

#[test]
fn unchanged_line_projections_survive_an_append() -> TestResult {
    for language in ["rust", "binary.zzz"] {
        let mut fixture = CacheBuilder::default().language(language).build();
        let first = fixture.append("fn first() {}\n")?;
        let second = fixture.append("fn second() {}\n")?;
        let before = first
            .highlights
            .line_shared(0)
            .ok_or("missing first line")?;
        let after = second
            .highlights
            .line_shared(0)
            .ok_or("missing first line")?;
        assert!(Arc::ptr_eq(&before, &after));
        assert_eq!(second.changed_lines, 1..2);
    }
    Ok(())
}

#[test]
fn raw_chunks_and_injections_match_complete_documents() -> TestResult {
    for (language, source) in [
        (
            "rust",
            "fn main() {\nlet café = r###\"open\nline\"###;\n}\n",
        ),
        (
            "python",
            "def run():\n    text = \"\"\"first\nsecond\"\"\"\n    return text\n",
        ),
        (
            "html",
            "<p>café</p><script>const x = 1;</script><style>b{color:red}</style>",
        ),
        ("rust", "/* open\n\nclose */\n"),
        ("markdown", "# Hello\n\n```rust\nfn main() {}\n```\n"),
        (
            "javascript",
            "function run() {\n  return `hello ${name}`;\n}\n",
        ),
        ("typescript", "const x: number = 1;\nconst y = false;\n"),
        ("binary.zzz", "café\r\n\nplain text\r\n"),
    ] {
        for chunk_chars in [1, 7, 256] {
            let mut fixture = CacheBuilder::default().language(language).build();
            let mut characters = source.chars();
            loop {
                let chunk: String = characters.by_ref().take(chunk_chars).collect();
                if chunk.is_empty() {
                    break;
                }
                fixture.append(&chunk)?;
                fixture.assert_equivalent()?;
            }
            assert_eq!(fixture.stream.source(), source);
        }
    }
    Ok(())
}

#[test]
fn eof_predicates_and_container_edits_match_reference() -> TestResult {
    for (language, source) in [
        (
            "rust",
            "fn main() { let VALUE = Some(123); println!(\"{VALUE}\"); }",
        ),
        (
            "python",
            "class Example:\n    def method(self):\n        return self.VALUE\n",
        ),
        ("javascript", "const value = /foo/g;\nconsole.log(value);\n"),
        ("bash", "echo \"${VALUE:-$(printf '%s' hello)}\"\n"),
        ("html", "<script>const text = `hello ${name}`;</script>\n"),
        ("markdown", "[label][ref]\n\n[ref]: https://example.com\n"),
        ("yaml", "key: |\n  value\nnext: true\n"),
        ("json", "{\"key\": true, \"nested\": [1, 2]}"),
    ] {
        let mut fixture = CacheBuilder::default().language(language).build();
        for character in source.chars() {
            fixture.append(character.encode_utf8(&mut [0; 4]))?;
            fixture.assert_equivalent()?;
        }
    }
    Ok(())
}

#[test]
fn nested_injections_obey_the_reference_depth_limit() -> TestResult {
    let mut source = String::from("const café = 123;\n");
    for depth in 0..6 {
        let fence = "`".repeat(depth + 3);
        let language = if depth == 0 { "javascript" } else { "markdown" };
        source = format!("{fence}{language}\n{source}{fence}\n");
        let mut fixture = CacheBuilder::default().language("markdown").build();
        let mut characters = source.chars();
        loop {
            let chunk: String = characters.by_ref().take(13).collect();
            if chunk.is_empty() {
                break;
            }
            fixture.append(&chunk)?;
            fixture.assert_equivalent()?;
        }
    }
    Ok(())
}

#[test]
fn injection_boundaries_can_close_and_be_reinterpreted() -> TestResult {
    for (language, chunks) in [
        (
            "markdown",
            vec![
                "```html\n<script>const café = 1;</script>\n",
                "```",
                "x\n",
                "```\n",
            ],
        ),
        (
            "html",
            vec![
                "<script>const café = `hello ${",
                "value}",
                "`;</script",
                ">tail<style>b{color:red}",
                "</style>",
            ],
        ),
    ] {
        let mut fixture = CacheBuilder::default().language(language).build();
        for chunk in chunks {
            fixture.append(chunk)?;
            fixture.assert_equivalent()?;
        }
        let before = fixture.stream.work_stats();
        fixture.append("")?;
        assert_eq!(fixture.stream.work_stats(), before);
    }
    Ok(())
}

#[test]
fn empty_updates_and_recoloring_do_not_parse() -> TestResult {
    let mut fixture = CacheBuilder::default().language("html").build();
    let first = fixture.append("<script>const café = 1;</script>")?;
    let work = fixture.stream.work_stats();
    fixture.highlighter.take_stats();
    let unchanged = fixture.append("")?;
    assert!(unchanged.changed_lines.is_empty());
    assert_eq!(unchanged.base_revision, unchanged.revision);
    assert!(Arc::ptr_eq(&first.highlights, &unchanged.highlights));
    assert_eq!(fixture.stream.work_stats(), work);
    assert_eq!(
        fixture.highlighter.take_stats(),
        HighlightStats {
            calls: 1,
            ..HighlightStats::default()
        }
    );
    fixture.theme = ReviewTheme::ayu()?.syntax;
    let recolored = fixture.append("")?;
    assert_eq!(recolored.revision, first.revision);
    assert_ne!(recolored.highlights.line(0), first.highlights.line(0));
    assert_eq!(fixture.highlighter.stats().bytes, 0);
    assert_eq!(
        fixture.stream.work_stats().parser_input_bytes,
        work.parser_input_bytes
    );
    assert_eq!(
        fixture.stream.work_stats().queried_bytes,
        work.queried_bytes
    );
    fixture.assert_equivalent()
}

#[test]
fn raw_input_limit_preserves_partial_line_and_work_state() -> TestResult {
    let mut fixture = CacheBuilder::default().source_limit(12).build();
    let accepted = fixture.append("let café")?;
    let work = fixture.stream.work_stats();
    let stats = fixture.highlighter.stats();
    for source in [" = 123;", "x\nthis line exceeds the budget"] {
        assert!(matches!(
            fixture.append(source),
            Err(SyntaxError::InputLimit { limit: 12, .. })
        ));
        assert_eq!(fixture.stream.source(), "let café");
        assert_eq!(fixture.stream.revision(), accepted.revision);
        assert!(Arc::ptr_eq(
            fixture.stream.highlights(),
            &accepted.highlights
        ));
        assert_eq!(fixture.stream.work_stats(), work);
        assert_eq!(fixture.highlighter.stats(), stats);
    }
    fixture.append("=1;")?;
    assert_eq!(fixture.stream.source(), "let café=1;");
    fixture.assert_equivalent()
}

#[test]
fn long_multiline_context_matches_complete_highlighting() -> TestResult {
    for (opening, closing) in [("let value = r###\"", "\"###;"), ("/*", "*/")] {
        let mut fixture = CacheBuilder::default().build();
        fixture.append(&format!("{opening}\n{}", "still inside\n".repeat(4096)))?;
        for chunk in ["let café = false;", closing, "fn done() {}"] {
            fixture.append(&format!("{chunk}\n"))?;
            fixture.assert_equivalent()?;
        }
    }
    Ok(())
}

#[test]
fn cloned_streams_append_independently() -> TestResult {
    let mut fixture = CacheBuilder::default().build();
    fixture.append("/* stable\n")?;
    let mut speculative = fixture.stream.clone();
    fixture
        .highlighter
        .with_theme(&fixture.theme)
        .append(&mut speculative, "closed */\n")?;
    fixture.append("still comment\n")?;
    assert!(!fixture.stream.source().contains("closed"));
    assert_eq!(speculative.source(), "/* stable\nclosed */\n");
    assert!(!line(fixture.stream.highlights(), 1)?.is_empty());
    fixture.assert_equivalent()
}

#[test]
fn changed_lines_include_restyled_earlier_content() -> TestResult {
    let mut fixture = CacheBuilder::default().build();
    let first = fixture.append("let value = r#\"open\n")?;
    let update = fixture.append("closed\"#;\n")?;
    for index in 0..update.highlights.line_count() {
        if first.highlights.line(index) != update.highlights.line(index) {
            assert!(update.changed_lines.contains(&index));
        }
    }
    fixture.assert_equivalent()
}

#[test]
fn split_shebangs_can_enable_syntax_after_plain_text() -> TestResult {
    let mut fixture = CacheBuilder::default().language("").build();
    for chunk in ["#!", "/usr/bin/env py", "thon3\n", "print('café')\n"] {
        fixture.append(chunk)?;
        fixture.assert_equivalent()?;
    }
    assert_eq!(fixture.stream.work_stats().full_parses, 1);
    assert!(!line(fixture.stream.highlights(), 1)?.is_empty());
    Ok(())
}

#[test]
fn line_boundaries_preserve_exact_text_and_projection_counts() -> TestResult {
    for language in ["rust", "binary.zzz"] {
        let mut fixture = CacheBuilder::default().language(language).build();
        for chunk in [
            "",
            "\n",
            "\n",
            "/* café",
            "\r",
            "\n",
            "still comment",
            "\r\n",
            "*/",
            "\n",
        ] {
            fixture.append(chunk)?;
            fixture.assert_equivalent()?;
        }
        let document = fixture.highlight(fixture.stream.source().to_owned().as_str())?;
        assert_eq!(
            document.line_count(),
            fixture.stream.highlights().line_count()
        );
    }
    Ok(())
}
