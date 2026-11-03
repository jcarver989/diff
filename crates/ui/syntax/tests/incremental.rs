mod support;

use clankerdiff_syntax::{MAX_SOURCE_BYTES, SyntaxError, analyze, thread_work};
use std::{fmt::Write, str, sync::Arc};
use support::{SyntaxBuilder, TestResult, line};

#[test]
fn growing_code_and_plain_text_have_linear_work() -> TestResult {
    for language in ["rust", "binary.zzz", ""] {
        let mut fixture = SyntaxBuilder::default().language(language).build();
        let chunk = "let value = 123;\n".repeat(16);
        for _ in 0..96 {
            fixture.append(&chunk)?;
        }
        let budget = 4 * fixture.stream.source().len() + 64 * 1024;
        let work = fixture.stream.work_stats();
        assert!(work.parser_input_bytes <= budget, "{language}: {work:?}");
        assert!(work.queried_bytes <= budget, "{language}: {work:?}");
        assert!(work.projected_bytes <= budget, "{language}: {work:?}");
        assert_eq!(work.full_parses, usize::from(language == "rust"));
        assert_eq!(work.incremental_parses, if language == "rust" { 95 } else { 0 });
        fixture.assert_equivalent();
    }
    Ok(())
}

#[test]
fn aether_code_fixture_meets_the_parser_and_query_budget() -> TestResult {
    let mut source = String::new();
    let mut line = 0;
    while source.len() < 24 * 1024 {
        writeln!(source, "let value_{line} = state.reconcile(incoming[{line}]).expect(\"delta accepted\");")?;
        line += 1;
    }
    let mut fixture = SyntaxBuilder::default().build();
    for chunk in source.as_bytes().chunks(256) {
        fixture.append(str::from_utf8(chunk)?)?;
    }
    let work = fixture.stream.work_stats();
    let budget = 4 * source.len() + 64 * 1024;
    assert!(work.parser_input_bytes <= budget, "{work:?}, budget {budget}");
    assert!(work.queried_bytes <= budget, "{work:?}, budget {budget}");
    assert!(work.projected_bytes <= budget, "{work:?}, budget {budget}");
    fixture.assert_equivalent();
    Ok(())
}

#[test]
fn aether_open_function_fixture_meets_the_parser_and_query_budget() -> TestResult {
    let mut fixture = SyntaxBuilder::default().build();
    fixture.append("fn main() {\n")?;
    let mut source = String::new();
    let mut line = 0;
    while source.len() < 24 * 1024 {
        writeln!(source, "let value_{line} = state.reconcile(incoming[{line}]).expect(\"delta accepted\");")?;
        line += 1;
    }
    for chunk in source.as_bytes().chunks(256) {
        fixture.append(str::from_utf8(chunk)?)?;
    }
    let work = fixture.stream.work_stats();
    let budget = 4 * fixture.stream.source().len() + 64 * 1024;
    assert!(work.parser_input_bytes <= budget, "{work:?}, budget {budget}");
    assert!(work.queried_bytes <= budget, "{work:?}, budget {budget}");
    assert!(work.projected_bytes <= budget, "{work:?}, budget {budget}");
    assert!(work.compared_nodes <= budget, "{work:?}, budget {budget}");
    fixture.assert_equivalent();
    fixture.append("}\n")?;
    fixture.assert_equivalent();
    Ok(())
}

#[test]
fn unchanged_line_projections_survive_an_append() -> TestResult {
    for language in ["rust", "binary.zzz"] {
        let mut fixture = SyntaxBuilder::default().language(language).build();
        let first = fixture.append("fn first() {}\n")?;
        let second = fixture.append("fn second() {}\n")?;
        assert_eq!(first.highlights.line(0), second.highlights.line(0));
        assert_eq!(second.changed_lines, 1..2);
    }
    Ok(())
}

#[test]
fn raw_chunks_and_injections_match_complete_documents() -> TestResult {
    for (language, source) in [
        ("rust", "fn main() {\nlet café = r###\"open\nline\"###;\n}\n"),
        ("python", "def run():\n    text = \"\"\"first\nsecond\"\"\"\n    return text\n"),
        ("html", "<p>café</p><script>const x = 1;</script><style>b{color:red}</style>"),
        ("rust", "/* open\n\nclose */\n"),
        ("markdown", "# Hello\n\n```rust\nfn main() {}\n```\n"),
        ("javascript", "function run() {\n  return `hello ${name}`;\n}\n"),
        ("typescript", "const x: number = 1;\nconst y = false;\n"),
        ("binary.zzz", "café\r\n\nplain text\r\n"),
    ] {
        for chunk_chars in [1, 7, 256] {
            let mut fixture = SyntaxBuilder::default().language(language).build();
            let mut characters = source.chars();
            loop {
                let chunk: String = characters.by_ref().take(chunk_chars).collect();
                if chunk.is_empty() {
                    break;
                }
                fixture.append(&chunk)?;
                fixture.assert_equivalent();
            }
            assert_eq!(fixture.stream.source(), source);
        }
    }
    Ok(())
}

#[test]
fn eof_predicates_and_container_edits_match_reference() -> TestResult {
    for (language, source) in [
        ("rust", "fn main() { let VALUE = Some(123); println!(\"{VALUE}\"); }"),
        ("python", "class Example:\n    def method(self):\n        return self.VALUE\n"),
        ("javascript", "const value = /foo/g;\nconsole.log(value);\n"),
        ("bash", "echo \"${VALUE:-$(printf '%s' hello)}\"\n"),
        ("html", "<script>const text = `hello ${name}`;</script>\n"),
        ("markdown", "[label][ref]\n\n[ref]: https://example.com\n"),
        ("yaml", "key: |\n  value\nnext: true\n"),
        ("json", "{\"key\": true, \"nested\": [1, 2]}"),
    ] {
        let mut fixture = SyntaxBuilder::default().language(language).build();
        for character in source.chars() {
            fixture.append(character.encode_utf8(&mut [0; 4]))?;
            fixture.assert_equivalent();
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
        let mut fixture = SyntaxBuilder::default().language("markdown").build();
        let mut characters = source.chars();
        loop {
            let chunk: String = characters.by_ref().take(13).collect();
            if chunk.is_empty() {
                break;
            }
            fixture.append(&chunk)?;
            fixture.assert_equivalent();
        }
    }
    Ok(())
}

#[test]
fn injection_boundaries_can_close_and_be_reinterpreted() -> TestResult {
    for (language, chunks) in [
        ("markdown", vec!["```html\n<script>const café = 1;</script>\n", "```", "x\n", "```\n"]),
        (
            "html",
            vec!["<script>const café = `hello ${", "value}", "`;</script", ">tail<style>b{color:red}", "</style>"],
        ),
    ] {
        let mut fixture = SyntaxBuilder::default().language(language).build();
        for chunk in chunks {
            fixture.append(chunk)?;
            fixture.assert_equivalent();
        }
        let before = fixture.stream.work_stats();
        fixture.append("")?;
        assert_eq!(fixture.stream.work_stats(), before);
    }
    Ok(())
}

#[test]
fn empty_updates_do_not_parse() -> TestResult {
    let mut fixture = SyntaxBuilder::default().language("html").build();
    let first = fixture.append("<script>const café = 1;</script>")?;
    let work = fixture.stream.work_stats();
    let unchanged = fixture.append("")?;
    assert!(unchanged.changed_lines.is_empty());
    assert_eq!(unchanged.base_revision, unchanged.revision);
    assert!(Arc::ptr_eq(&first.highlights, &unchanged.highlights));
    assert_eq!(fixture.stream.work_stats(), work);
    fixture.assert_equivalent();
    Ok(())
}

#[test]
fn complete_analysis_parses_once_and_matches_streaming() -> TestResult {
    let mut fixture = SyntaxBuilder::default().build();
    let source = "/* open\nclosed */\nfn café() {}\n";
    let before = thread_work();
    let analysis = analyze("rust", source);
    let after = thread_work();
    fixture.append(source)?;
    assert_eq!(after.full_parses - before.full_parses, 1);
    assert_eq!(after.incremental_parses, before.incremental_parses);
    assert_eq!(after.parser_input_bytes - before.parser_input_bytes, fixture.stream.work_stats().parser_input_bytes);
    assert_eq!(analysis.highlights.line_count(), 3);
    for index in 0..analysis.highlights.line_count() {
        assert_eq!(analysis.highlights.line(index), fixture.stream.highlights().line(index));
    }
    assert_eq!(analysis.symbols, fixture.stream.syntax().symbols);
    assert_eq!(analysis.symbols.definitions.len(), 1);
    Ok(())
}

#[test]
fn raw_input_limit_preserves_partial_line_and_work_state() -> TestResult {
    let mut fixture = SyntaxBuilder::default().build();
    let accepted = fixture.append("let café")?;
    let work = fixture.stream.work_stats();
    for source in [" ".repeat(MAX_SOURCE_BYTES), format!("x\n{}", "y".repeat(MAX_SOURCE_BYTES))] {
        assert!(matches!(fixture.append(&source), Err(SyntaxError::InputLimit { limit: MAX_SOURCE_BYTES, .. })));
        assert_eq!(fixture.stream.source(), "let café");
        assert_eq!(fixture.stream.revision(), accepted.revision);
        assert!(Arc::ptr_eq(fixture.stream.highlights(), &accepted.highlights));
        assert_eq!(fixture.stream.work_stats(), work);
    }
    fixture.append("=1;")?;
    assert_eq!(fixture.stream.source(), "let café=1;");
    fixture.assert_equivalent();
    Ok(())
}

#[test]
fn long_multiline_context_matches_complete_highlighting() -> TestResult {
    for (opening, closing) in [("let value = r###\"", "\"###;"), ("/*", "*/")] {
        let mut fixture = SyntaxBuilder::default().build();
        fixture.append(&format!("{opening}\n{}", "still inside\n".repeat(4096)))?;
        for chunk in ["let café = false;", closing, "fn done() {}"] {
            fixture.append(&format!("{chunk}\n"))?;
            fixture.assert_equivalent();
        }
    }
    Ok(())
}

#[test]
fn cloned_streams_append_independently() -> TestResult {
    let mut fixture = SyntaxBuilder::default().build();
    fixture.append("/* stable\n")?;
    let mut speculative = fixture.stream.clone();
    speculative.append("closed */\n")?;
    fixture.append("still comment\n")?;
    assert!(!fixture.stream.source().contains("closed"));
    assert_eq!(speculative.source(), "/* stable\nclosed */\n");
    assert!(!line(fixture.stream.highlights(), 1)?.is_empty());
    fixture.assert_equivalent();
    Ok(())
}

#[test]
fn changed_lines_include_restyled_earlier_content() -> TestResult {
    let mut fixture = SyntaxBuilder::default().build();
    let first = fixture.append("let value = r#\"open\n")?;
    let update = fixture.append("closed\"#;\n")?;
    for index in 0..update.highlights.line_count() {
        if first.highlights.line(index) != update.highlights.line(index) {
            assert!(update.changed_lines.contains(&index));
        }
    }
    fixture.assert_equivalent();
    Ok(())
}

#[test]
fn split_shebangs_can_enable_syntax_after_plain_text() -> TestResult {
    let mut fixture = SyntaxBuilder::default().language("").build();
    for chunk in ["#!", "/usr/bin/env py", "thon3\n", "print('café')\n"] {
        fixture.append(chunk)?;
        fixture.assert_equivalent();
    }
    assert_eq!(fixture.stream.work_stats().full_parses, 1);
    assert!(!line(fixture.stream.highlights(), 1)?.is_empty());
    Ok(())
}

#[test]
fn line_boundaries_preserve_exact_text_and_projection_counts() -> TestResult {
    for language in ["rust", "binary.zzz"] {
        let mut fixture = SyntaxBuilder::default().language(language).build();
        for chunk in ["", "\n", "\n", "/* café", "\r", "\n", "still comment", "\r\n", "*/", "\n"] {
            fixture.append(chunk)?;
            fixture.assert_equivalent();
        }
        let document = fixture.highlight(fixture.stream.source().to_owned().as_str());
        assert_eq!(document.line_count(), fixture.stream.highlights().line_count());
    }
    Ok(())
}
