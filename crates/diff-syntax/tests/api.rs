use clankerdiff_syntax::{
    CacheConfig, DocumentHighlights, HighlightSpan, HighlightStats, LanguageHint, SourceSequenceId,
    SyntaxHighlighter, SyntaxStream, SyntaxStreamError,
};
use clankerdiff_theme::SyntaxTheme;
use std::{error::Error, fmt::Write, sync::Arc};

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
    let changed_theme = SyntaxTheme::ayu()?;
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
        let error = highlighter
            .with_theme(&theme)
            .append(&mut stream, lines)
            .expect_err("oversized input");
        assert!(matches!(
            error,
            SyntaxStreamError::InputLimit { limit: 16, .. }
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

fn line(highlights: &DocumentHighlights, index: usize) -> Result<&[HighlightSpan], Box<dyn Error>> {
    highlights
        .line(index)
        .ok_or_else(|| format!("missing line {index}").into())
}
