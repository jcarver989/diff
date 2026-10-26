#![cfg(feature = "test-support")]

use clankerdiff_markdown::MarkdownStream;
use clankerdiff_ratatui::{
    MarkdownCommitError, MarkdownPresentation, MarkdownRow, MarkdownStreamError,
    StreamingMarkdownPolicy, StreamingMarkdownState, testing::MarkdownStreamFixture,
};
use clankerdiff_theme::ReviewTheme;
use std::{error::Error, fmt::Write, sync::Arc};

type TestResult = Result<(), Box<dyn Error>>;

const HEIGHT: usize = 12;

#[test]
fn commit_requests_are_validated() -> TestResult {
    let mut reflowable = MarkdownStreamFixture::from_source("one\n\ntwo\n");
    reflowable.apply_update();
    assert_eq!(
        reflowable.state.commit_rows(reflowable.revision, 1),
        Err(MarkdownCommitError::Reflowable)
    );
    assert_eq!(
        StreamingMarkdownState::default().policy(),
        StreamingMarkdownPolicy::Reflowable
    );

    let mut fixture = MarkdownStreamFixture::terminal();
    assert_eq!(fixture.state.policy(), StreamingMarkdownPolicy::Terminal);
    fixture.stream.push("one\n\ntwo\n\nthree\n");
    fixture.apply_update();
    let revision = fixture.revision;
    let rows = fixture.host.len();
    assert_eq!(
        fixture.state.commit_rows(revision + 1, 1),
        Err(MarkdownCommitError::StaleRevision {
            base: revision + 1,
            revision
        })
    );
    assert_eq!(
        fixture.state.commit_rows(revision, rows + 1),
        Err(MarkdownCommitError::OutOfRange {
            rows,
            requested: rows + 1
        })
    );
    fixture.state.commit_rows(revision, 2)?;
    assert_eq!(fixture.state.committed_rows(), 2);
    assert_eq!(
        fixture.state.commit_rows(revision, 1),
        Err(MarkdownCommitError::Decreasing {
            committed: 2,
            requested: 1
        })
    );
    fixture.state.commit_rows(revision, 2)?;
    fixture.state.commit_rows(revision, rows)?;
    assert_eq!(fixture.state.committed_rows(), rows);
    fixture.stream.push("\nfour\n");
    fixture.assert_equivalent();
    assert!(fixture.host.len() > rows);
    Ok(())
}

#[test]
fn prefix_bookkeeping_scales_with_changed_blocks_not_history() -> TestResult {
    for terminal in [false, true] {
        for count in [128, 256, 512] {
            let mut fixture = if terminal {
                MarkdownStreamFixture::terminal()
            } else {
                MarkdownStreamFixture::default()
            };
            for index in 0..count {
                fixture.stream.push(&format!("paragraph {index}\n\n"));
                fixture.apply_update();
                if terminal {
                    fixture.commit_overflow(HEIGHT)?;
                }
            }
            let work = fixture.state.take_stats();
            assert!(work.blocks_visited <= 4 * count, "{work:?}");
            assert!(work.chunks_visited <= 16 * count, "{work:?}");
            assert!(work.row_store_updates <= 32 * count, "{work:?}");
            fixture.assert_output_equivalent();
        }
    }
    Ok(())
}

#[test]
fn cached_updates_never_cross_a_new_commit_boundary() -> TestResult {
    let mut fixture = MarkdownStreamFixture::terminal();
    fixture.stream.push("one\n\ntwo\n\nthree\n");
    let update = fixture.apply_update();
    for committed in 1..=fixture.host.len() {
        fixture.state.commit_rows(fixture.revision, committed)?;
        for base in [update.base_revision, fixture.revision, fixture.revision + 1] {
            let delta = fixture.state.update_since(base);
            assert!(delta.first_changed_row >= committed, "{delta:?}");
            assert!(!delta.reset);
            assert_eq!(delta.total_rows(), fixture.host.len());
            assert!(
                delta
                    .replacement
                    .iter()
                    .eq(fixture.host[delta.first_changed_row..].iter())
            );
        }
    }
    Ok(())
}

#[test]
fn committed_rows_stay_frozen_across_finish_resize_and_theme_changes() -> TestResult {
    let mut fixture = MarkdownStreamFixture::terminal();
    fixture.options.width = 30;
    for chunk in chunk_message(&exactness_message(), 61) {
        fixture.stream.push(&chunk);
        fixture.apply_update();
        fixture.commit_overflow(HEIGHT)?;
    }
    let committed = fixture.state.committed_rows();
    assert!(committed > HEIGHT);
    let frozen: Vec<Arc<MarkdownRow>> = fixture.host[..committed].to_vec();

    fixture.stream.finish();
    let update = fixture.apply_update();
    assert!(update.first_changed_row >= committed);
    assert_frozen(&mut fixture, &frozen);

    fixture.options.width = 50;
    let update = fixture.apply_update();
    assert!(update.first_changed_row >= committed);
    assert!(!update.reset);
    assert_frozen(&mut fixture, &frozen);
    assert!(
        fixture.host[committed..]
            .iter()
            .any(|row| row.line.width() > 30)
    );

    fixture.theme = ReviewTheme::ayu()?;
    let update = fixture.apply_update();
    assert!(update.first_changed_row >= committed);
    assert_frozen(&mut fixture, &frozen);

    let stale = fixture.state.update_since(0);
    assert_eq!(stale.first_changed_row, committed);
    assert!(!stale.reset);
    assert!(
        stale
            .replacement
            .iter()
            .eq(fixture.host[committed..].iter())
    );
    Ok(())
}

#[test]
fn terminal_output_matches_one_shot_for_append_safe_fixtures() -> TestResult {
    let message = format!("{}\n{}", prose_message(4096), code_block_message(4096));
    for chunk_bytes in [7, 37, 128, 1024] {
        let mut fixture = MarkdownStreamFixture::terminal();
        fixture.options.width = 120;
        for chunk in chunk_message(&message, chunk_bytes) {
            fixture.stream.push(&chunk);
            fixture.assert_output_equivalent();
            fixture.commit_overflow(HEIGHT)?;
            assert!(fixture.host.len() - fixture.state.committed_rows() <= HEIGHT + 1);
        }
        fixture.stream.finish();
        fixture.assert_output_equivalent();
        assert!(fixture.state.committed_rows() > 3 * HEIGHT);
    }
    Ok(())
}

#[test]
fn rows_kept_live_after_a_failed_write_can_be_committed_later() -> TestResult {
    let mut fixture = MarkdownStreamFixture::terminal();
    fixture.stream.push("one\n\ntwo\n\nthree\n\n");
    fixture.apply_update();
    let before = fixture.host.clone();
    fixture.stream.push("four\n\n");
    let update = fixture.apply_update();
    assert_eq!(update.first_changed_row, before.len());
    assert_eq!(fixture.state.committed_rows(), 0);
    fixture.state.commit_rows(fixture.revision, 3)?;
    fixture.stream.push("five\n");
    fixture.assert_equivalent();
    assert!(
        fixture.host[..3]
            .iter()
            .zip(&before)
            .all(|(a, b)| Arc::ptr_eq(a, b))
    );
    Ok(())
}

#[test]
fn equal_rows_keep_host_identity_when_a_fence_closes() -> TestResult {
    let mut fixture = MarkdownStreamFixture::terminal();
    fixture.stream.push("```rust\nlet a = 1;\nlet b = 2;");
    fixture.apply_update();
    let frozen = fixture.host[..1].to_vec();
    fixture.stream.push("\n```\n\nafter");
    fixture.assert_equivalent();
    let layout = fixture.layout();
    assert!(
        fixture
            .host
            .iter()
            .zip(layout.rows().iter())
            .all(|(host, row)| Arc::ptr_eq(host, row))
    );
    fixture.state.commit_rows(fixture.revision, 1)?;
    assert_frozen(&mut fixture, &frozen);
    fixture.stream.push(" another paragraph");
    fixture.apply_update();
    assert_frozen(&mut fixture, &frozen);
    Ok(())
}

#[test]
fn late_markdown_reinterpretation_keeps_history_and_continues_after_it() -> TestResult {
    let mut fixture = MarkdownStreamFixture::terminal();
    fixture
        .stream
        .push("See [the docs] now.\n\nSecond paragraph.\n\nThird.\n");
    fixture.apply_update();
    fixture.state.commit_rows(fixture.revision, 3)?;
    let frozen = fixture.host[..3].to_vec();
    fixture.stream.push("\n[the docs]: https://example.com\n");
    let update = fixture.apply_update();
    assert!(update.first_changed_row >= 3);
    assert_frozen(&mut fixture, &frozen);
    assert!(!fixture.host[0].line.spans.iter().any(|span| {
        span.style
            .add_modifier
            .contains(ratatui::style::Modifier::UNDERLINED)
    }));
    let one_shot = fixture.one_shot();
    assert!(
        one_shot
            .row(0)
            .is_some_and(|row| row.line.spans.iter().any(|span| {
                span.style
                    .add_modifier
                    .contains(ratatui::style::Modifier::UNDERLINED)
            }))
    );
    assert!(fixture.host[3..].iter().eq(one_shot.rows().iter().skip(3)));
    Ok(())
}

#[test]
fn a_vanished_block_at_the_boundary_neither_duplicates_nor_loses_source() -> TestResult {
    let mut fixture = MarkdownStreamFixture::terminal();
    fixture.stream.push("[x]:");
    fixture.apply_update();
    assert_eq!(fixture.host.len(), 1);
    fixture.state.commit_rows(fixture.revision, 1)?;
    fixture.stream.push("\n/url\n\nafter\n");
    let update = fixture.apply_update();
    assert!(update.first_changed_row >= 1);
    let text = fixture
        .host
        .iter()
        .map(|row| row.line.to_string())
        .collect::<Vec<_>>();
    assert_eq!(text[0], "[x]:");
    assert_eq!(text.iter().filter(|line| line.contains("after")).count(), 1);
    assert_eq!(text.iter().filter(|line| line.contains("[x]:")).count(), 1);
    Ok(())
}

#[test]
fn replacing_committed_source_is_rejected() -> TestResult {
    let mut fixture = MarkdownStreamFixture::terminal();
    fixture.stream.push("one\n\ntwo\n\nthree\n");
    fixture.apply_update();
    fixture.state.commit_rows(fixture.revision, 2)?;
    fixture.stream.replace("something else\n");
    assert_eq!(
        fixture.try_layout().map(|_| ()),
        Err(MarkdownStreamError::CommittedSourceReplaced { committed: 2 })
    );
    fixture.stream = MarkdownStream::new();
    fixture.stream.push("fresh\n");
    assert_eq!(
        fixture.try_layout().map(|_| ()),
        Err(MarkdownStreamError::CommittedSourceReplaced { committed: 2 })
    );
    let mut reflowable = MarkdownStreamFixture::from_source("one\n\ntwo\n");
    reflowable.apply_update();
    reflowable.stream.replace("replaced\n");
    let update = reflowable.apply_update();
    assert!(update.reset);
    assert_eq!(update.first_changed_row, 0);
    reflowable.assert_equivalent();
    Ok(())
}

#[test]
fn reset_preserves_committed_history() -> TestResult {
    let mut fixture = MarkdownStreamFixture::terminal();
    fixture.stream.push("one\n\ntwo\n\nthree\n");
    fixture.apply_update();
    fixture.state.commit_rows(fixture.revision, 3)?;
    let frozen = fixture.host[..3].to_vec();
    fixture.state.reset();
    assert_eq!(fixture.state.committed_rows(), 3);
    fixture.stream.push("\nfour\n");
    let update = fixture.apply_update();
    assert_eq!(update.first_changed_row, 3);
    assert!(!update.reset);
    assert_frozen(&mut fixture, &frozen);
    assert!(
        fixture
            .host
            .iter()
            .any(|row| row.line.to_string() == "four")
    );
    Ok(())
}

#[test]
fn open_code_blocks_reuse_rows_and_highlight_incrementally() -> TestResult {
    let source = code_block_message(24 * 1024);
    for policy in [
        StreamingMarkdownPolicy::Reflowable,
        StreamingMarkdownPolicy::Terminal,
    ] {
        let mut fixture = MarkdownStreamFixture {
            state: StreamingMarkdownState::new(policy),
            ..MarkdownStreamFixture::default()
        };
        fixture.options.width = 120;
        let mut previous: Vec<Arc<MarkdownRow>> = Vec::new();
        for chunk in chunk_message(&source, 256) {
            fixture.stream.push(&chunk);
            fixture.assert_equivalent();
            let shared = previous
                .iter()
                .zip(&fixture.host)
                .take(previous.len().saturating_sub(1))
                .all(|(a, b)| Arc::ptr_eq(a, b));
            assert!(shared, "unchanged code rows must keep their identity");
            previous.clone_from(&fixture.host);
            if policy == StreamingMarkdownPolicy::Terminal {
                fixture.commit_overflow(HEIGHT)?;
            }
        }
        fixture.stream.finish();
        fixture.assert_equivalent();
        let work = fixture.state.take_stats();
        let budget = 4 * source.len() + 64 * 1024;
        assert!(work.parsed_bytes <= budget, "{work:?}");
        assert!(work.highlighted_bytes <= budget, "{work:?}");
        assert!(work.scanned_bytes <= budget, "{work:?}");
        let rows = fixture.host.len();
        assert!(
            work.rows_generated <= 4 * rows + 64,
            "{work:?}, rows {rows}"
        );
        assert!(work.rows_compared <= 4 * rows + 64, "{work:?}, rows {rows}");
        assert_eq!(work.rows_materialized, 0);
    }
    Ok(())
}

#[test]
fn live_region_stays_bounded_while_streaming_prose_and_code() -> TestResult {
    for (source, code) in [
        (prose_message(16 * 1024), false),
        (code_block_message(24 * 1024), true),
    ] {
        let mut fixture = MarkdownStreamFixture::terminal();
        fixture.options.width = 120;
        let mut max_live = 0;
        for chunk in chunk_message(&source, 256) {
            fixture.stream.push(&chunk);
            fixture.apply_update();
            fixture.commit_overflow(HEIGHT)?;
            max_live = max_live.max(fixture.host.len() - fixture.state.committed_rows());
        }
        fixture.stream.finish();
        fixture.assert_output_equivalent();
        assert!(
            max_live <= 2 * HEIGHT,
            "live region reached {max_live} rows"
        );
        let work = fixture.state.take_stats();
        let budget = 4 * source.len() + 64 * 1024;
        assert!(work.parsed_bytes <= budget, "{work:?}");
        assert!(!code || work.highlighted_bytes <= budget, "{work:?}");
        fixture.state.take_stats();
        fixture.apply_update();
        let settled = fixture.state.take_stats();
        assert_eq!(settled.parsed_bytes, 0);
        assert_eq!(settled.highlighted_bytes, 0);
        assert_eq!(settled.rows_generated, 0);
    }
    Ok(())
}

#[test]
fn retroactive_code_styles_only_change_uncommitted_rows() -> TestResult {
    for chunk_bytes in [7, 37, 128, 1024] {
        let mut terminal = MarkdownStreamFixture::terminal();
        let mut reflowable = MarkdownStreamFixture::default();
        terminal.options.width = 60;
        reflowable.options.width = 60;
        for chunk in chunk_message(&exactness_message(), chunk_bytes) {
            let committed = terminal.state.committed_rows();
            let frozen = terminal.host[..committed].to_vec();
            terminal.stream.push(&chunk);
            reflowable.stream.push(&chunk);
            reflowable.assert_equivalent();
            terminal.apply_update();
            assert_frozen(&mut terminal, &frozen);
            assert_eq!(terminal.host.len(), reflowable.host.len());
            for (index, (actual, expected)) in
                terminal.host.iter().zip(&reflowable.host).enumerate()
            {
                assert_eq!(
                    actual.line.to_string(),
                    expected.line.to_string(),
                    "row {index}"
                );
                if index >= committed {
                    assert_eq!(actual.line, expected.line, "live row {index}");
                }
            }
            terminal.commit_overflow(HEIGHT)?;
        }
        let frozen = terminal.host[..terminal.state.committed_rows()].to_vec();
        terminal.stream.finish();
        terminal.apply_update();
        assert_frozen(&mut terminal, &frozen);
    }
    Ok(())
}

#[test]
fn partial_row_checkpoints_survive_repeated_resizes_and_commits() -> TestResult {
    for source in [
        "```text\n0123456789abcdefghijklmnopqrstuvwxyz",
        "0123456789abcdefghijklmnopqrstuvwxyz",
        "```text\n\t0123456789abcdefghijklmnopqrstuvwxyz",
    ] {
        let mut fixture = MarkdownStreamFixture::terminal();
        fixture.options.width = 3;
        fixture.stream.push(source);
        fixture.apply_update();
        let original = fixture
            .host
            .iter()
            .map(|row| row.line.to_string())
            .collect::<String>();
        fixture.state.commit_rows(fixture.revision, 1)?;
        for width in [7, 2, 11] {
            let frozen = fixture.host[..fixture.state.committed_rows()].to_vec();
            fixture.options.width = width;
            fixture.apply_update();
            assert_frozen(&mut fixture, &frozen);
            assert_eq!(
                fixture
                    .host
                    .iter()
                    .map(|row| row.line.to_string())
                    .collect::<String>(),
                original
            );
            fixture
                .state
                .commit_rows(fixture.revision, fixture.state.committed_rows() + 1)?;
        }
    }
    Ok(())
}

#[test]
fn hard_break_checkpoint_resumes_with_the_quote_prefix() -> TestResult {
    let mut fixture = MarkdownStreamFixture::terminal();
    fixture.options.width = 4;
    fixture.stream.push("> a  \n> bcdef");
    fixture.apply_update();
    let text = |rows: &[Arc<MarkdownRow>]| {
        rows.iter()
            .map(|row| row.line.to_string())
            .collect::<Vec<_>>()
    };
    fixture.state.commit_rows(fixture.revision, 1)?;
    fixture.stream.push("g");
    fixture.apply_update();
    assert_eq!(text(&fixture.host), ["│ a", "│ bc", "│ de", "│ fg"]);
    Ok(())
}

#[test]
fn partial_tab_checkpoint_survives_append_and_resize() -> TestResult {
    let mut fixture = MarkdownStreamFixture::terminal();
    fixture.options.presentation = MarkdownPresentation::SourceLines;
    fixture.options.width = 2;
    fixture.options.tab_width = 8;
    fixture.stream.push("a\tb");
    fixture.apply_update();
    fixture.state.commit_rows(fixture.revision, 1)?;
    for (width, suffix, expected) in [(3, "c", "a       bc"), (1, "d\n", "a       bcd")] {
        fixture.options.width = width;
        fixture.stream.push(suffix);
        fixture.apply_update();
        assert_eq!(
            fixture
                .host
                .iter()
                .map(|row| row.line.to_string())
                .collect::<String>(),
            expected
        );
    }
    Ok(())
}

#[test]
fn source_line_caches_follow_width_and_theme_changes() -> TestResult {
    let mut fixture = MarkdownStreamFixture::from_source(
        "# Heading\n\nA **styled** paragraph with enough text to wrap.\n\n```rust\nlet value = 123;\n```\n",
    );
    fixture.options.presentation = MarkdownPresentation::SourceLines;
    fixture.assert_equivalent();
    for width in [7, 40, 12] {
        fixture.options.width = width;
        fixture.assert_equivalent();
        fixture.theme = ReviewTheme::ayu()?;
        fixture.assert_equivalent();
        fixture.theme = ReviewTheme::default();
        fixture.assert_equivalent();
    }
    fixture.stream.push("\nFinal **tail**.\n");
    fixture.assert_equivalent();
    Ok(())
}

#[test]
fn source_line_checkpoints_preserve_text_on_resize_and_append() -> TestResult {
    let mut fixture = MarkdownStreamFixture::terminal();
    fixture.options.presentation = MarkdownPresentation::SourceLines;
    fixture.options.width = 3;
    fixture
        .stream
        .push("abcdefghijklmnop\n\n```rust\nlet value = 1;\n```\n");
    fixture.apply_update();
    fixture.state.commit_rows(fixture.revision, 1)?;
    let frozen = fixture.host[..1].to_vec();
    fixture.options.width = 7;
    fixture.stream.push("\n**tail**");
    fixture.apply_update();
    assert_frozen(&mut fixture, &frozen);
    let one_shot = fixture.one_shot();
    assert_eq!(
        fixture
            .host
            .iter()
            .map(|row| row.line.to_string())
            .collect::<String>(),
        one_shot
            .rows()
            .iter()
            .map(|row| row.line.to_string())
            .collect::<String>()
    );
    Ok(())
}

#[test]
fn inline_reinterpretation_does_not_drop_the_live_suffix() -> TestResult {
    let mut fixture = MarkdownStreamFixture::terminal();
    fixture.options.width = 5;
    fixture.stream.push("**abcdefghij");
    fixture.apply_update();
    fixture.state.commit_rows(fixture.revision, 1)?;
    let frozen = fixture.host[..1].to_vec();
    fixture.stream.push("** tail");
    fixture.apply_update();
    assert_frozen(&mut fixture, &frozen);
    assert_eq!(
        fixture
            .host
            .iter()
            .map(|row| row.line.to_string())
            .collect::<String>(),
        "**abcdefghij tail"
    );
    Ok(())
}

#[test]
fn structured_boundaries_and_final_commits_preserve_history() -> TestResult {
    for source in [
        "1. alpha one two three\n2. beta four five six\n3. gamma seven eight nine\n",
        "| name | value |\n| --- | --- |\n| alpha | one two three |\n| beta | four five six |\n",
        "> alpha one two three\n> beta four five six\n",
        "- parent\n\n  ```rust\n  let alpha = 1;\n  let beta = 2;\n  ```\n",
    ] {
        for commit in [1, 2, 3] {
            let mut fixture = MarkdownStreamFixture::terminal();
            fixture.options.width = 16;
            fixture.stream.push(source);
            fixture.apply_update();
            let commit = commit.min(fixture.host.len());
            fixture.state.commit_rows(fixture.revision, commit)?;
            let frozen = fixture.host[..commit].to_vec();
            fixture.options.width = 24;
            fixture.theme = ReviewTheme::ayu()?;
            fixture.stream.push("\n\nfinal sentinel\n");
            fixture.apply_update();
            assert_frozen(&mut fixture, &frozen);
            assert_eq!(
                fixture
                    .host
                    .iter()
                    .filter(|row| row.line.to_string().contains("final sentinel"))
                    .count(),
                1
            );
            fixture.stream.finish();
            fixture.apply_update();
            fixture
                .state
                .commit_rows(fixture.revision, fixture.host.len())?;
            let frozen = fixture.host.clone();
            for width in [8, 40] {
                fixture.options.width = width;
                fixture.theme = ReviewTheme::default();
                fixture.apply_update();
                assert_frozen(&mut fixture, &frozen);
                assert_eq!(fixture.host.len(), frozen.len());
            }
        }
    }
    Ok(())
}

#[test]
fn growing_fence_bookkeeping_reuses_its_row_prefix() -> TestResult {
    for terminal in [false, true] {
        let mut fixture = if terminal {
            MarkdownStreamFixture::terminal()
        } else {
            MarkdownStreamFixture::default()
        };
        fixture.stream.push("intro\n\n```rust\n");
        for _ in 0..512 {
            fixture.stream.push("let value = 123;\n");
            fixture.apply_update();
            if terminal {
                fixture.commit_overflow(HEIGHT)?;
            }
        }
        let work = fixture.state.take_stats();
        assert!(work.targets_visited < 512 * 16, "{work:?}");
        assert!(work.chunks_visited < 512 * 16, "{work:?}");
        assert!(work.rows_compared < 512 * 16, "{work:?}");
        assert!(work.row_store_updates < 512 * 32, "{work:?}");
        fixture.assert_output_equivalent();
    }
    Ok(())
}

fn assert_frozen(fixture: &mut MarkdownStreamFixture, frozen: &[Arc<MarkdownRow>]) {
    assert!(fixture.host.len() >= frozen.len());
    assert!(
        fixture
            .host
            .iter()
            .zip(frozen)
            .all(|(a, b)| Arc::ptr_eq(a, b)),
        "committed rows changed identity"
    );
    let layout = fixture.layout();
    assert!(
        layout
            .rows()
            .iter()
            .zip(frozen)
            .all(|(a, b)| Arc::ptr_eq(a, b)),
        "layout no longer starts with the committed rows"
    );
    assert_eq!(fixture.state.committed_rows(), frozen.len());
}

fn exactness_message() -> String {
    let mut message = String::from("Streaming contract\n===\n\n");
    message.push_str(
        "The paragraph above is a setext heading, so no line of it may be finalized early.\n\n",
    );
    message.push_str("```rust\nfn one() {\n    let s = \"starts here\n");
    for index in 0..40 {
        let _ = writeln!(message, "and continues {index}");
    }
    message.push_str("    ```\n");
    message.push_str("ends here\";\n\nlet two = one();\n}\n```\n\n");
    message.push_str("Between blocks.\n\n```python\ndef f():\n    doc = \"\"\"First line\n");
    for index in 0..40 {
        let _ = writeln!(message, "doc line {index}");
    }
    message.push_str("last line\"\"\"\n    return doc\n```\n\n");
    message.push_str("~~~text\nthis ~~ fence holds ``` backtick ``` lines inside\n~~~\n\n");
    message.push_str("```js\nlet first = 1;\n```\n```text\nsecond\n```\n\n");
    message.push_str("Tail paragraph before the still-open block.\n\n```rust\nfn unclosed(");
    message.push_str(&"x".repeat(600));
    message
}

fn prose_message(total_bytes: usize) -> String {
    let mut message = String::new();
    let mut sentence = 0;
    while message.len() < total_bytes {
        for _ in 0..4 {
            let _ = write!(
                message,
                "Sentence {sentence} carries ordinary words so wrapping and parsing do real work. "
            );
            sentence += 1;
        }
        message.push_str("\n\n");
    }
    message
}

fn code_block_message(total_bytes: usize) -> String {
    let mut message = String::from("```rust\n");
    let mut line = 0;
    while message.len() < total_bytes {
        let _ = writeln!(
            message,
            "let value_{line} = state.reconcile(incoming[{line}]).expect(\"delta accepted\");"
        );
        line += 1;
    }
    message.push_str("```\n");
    message
}

fn chunk_message(message: &str, chunk_bytes: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut rest = message;
    while !rest.is_empty() {
        let mut end = rest.len().min(chunk_bytes);
        while !rest.is_char_boundary(end) {
            end += 1;
        }
        chunks.push(rest[..end].to_owned());
        rest = &rest[end..];
    }
    chunks
}
