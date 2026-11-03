use clankerdiff_markdown::MarkdownStream;
use clankerdiff_syntax::{analyze, thread_work};
use std::error::Error;

#[test]
fn source_updates_report_byte_ranges_without_promising_stable_prefixes() {
    let mut stream = MarkdownStream::new();
    let first = stream.push("hello\n\n");
    assert_eq!(first.revision, 1);
    assert_eq!(first.changed_source, 0..7);
    let second = stream.push("世界");
    assert_eq!(second.revision, 2);
    assert_eq!(second.changed_source, 7..13);
    assert_eq!(stream.source(), "hello\n\n世界");
    let finished = stream.finish();
    assert_eq!(finished.revision, 3);
    assert_eq!(finished.changed_source, 13..13);
    assert!(finished.finished);
    assert!(stream.is_finished());
}

#[test]
fn operations_that_change_nothing_keep_the_revision() {
    let mut stream = MarkdownStream::new();
    stream.push("text");
    let revision = stream.revision();
    let empty = stream.push("");
    assert_eq!(empty.revision, revision);
    assert!(empty.changed_source.is_empty());
    assert!(!empty.reset);
    stream.finish();
    let again = stream.finish();
    assert_eq!(again.revision, revision + 1);
    assert!(again.finished);
    assert_eq!(stream.revision(), revision + 1);
    let resumed = stream.push("");
    assert_eq!(resumed.revision, revision + 2);
    assert!(resumed.reset);
    assert!(!stream.is_finished());
}

#[test]
fn replacement_and_appending_after_completion_report_resets() {
    let mut stream = MarkdownStream::new();
    stream.push("```rust\nopen\n");
    let identity = stream.identity();
    let replaced = stream.replace("x");
    assert!(replaced.reset);
    assert_eq!(replaced.changed_source, 0..1);
    assert_eq!(stream.source(), "x");
    assert_eq!(stream.identity(), identity);
    stream.finish();
    let resumed = stream.push("é");
    assert!(resumed.reset);
    assert!(!resumed.finished);
    assert_eq!(resumed.changed_source, 1..3);
    assert_eq!(stream.source(), "xé");
}

#[test]
fn source_revisions_only_advance_when_source_can_change() {
    let mut stream = MarkdownStream::new();
    assert_eq!(stream.source_revision(), 0);
    stream.push("café");
    let source_revision = stream.source_revision();
    assert_eq!(source_revision, 1);
    stream.push("");
    stream.finish();
    stream.finish();
    stream.push("");
    assert_eq!(stream.source_revision(), source_revision);
    assert_eq!(stream.clone().source_revision(), source_revision);
    stream.replace("café");
    assert_eq!(stream.source_revision(), source_revision + 1);
    stream.push("!");
    assert_eq!(stream.source_revision(), source_revision + 2);
}

#[test]
fn independent_and_cloned_streams_have_distinct_identities() {
    let mut first = MarkdownStream::new();
    first.push("before");
    let mut cloned = first.clone();
    let second = MarkdownStream::new();
    assert_ne!(first.identity(), second.identity());
    assert_ne!(first.identity(), cloned.identity());
    assert_eq!(first.revision(), cloned.revision());
    assert_eq!(first.source(), cloned.source());
    first.push(" first");
    cloned.push(" second");
    assert_eq!(first.revision(), cloned.revision());
    assert_ne!(first.source(), cloned.source());
}

#[test]
fn an_open_code_block_is_highlighted_as_it_streams_and_keeps_that_analysis_when_closed() -> Result<(), Box<dyn Error>> {
    let code = "fn main() {\n    let a = 1;\n}";
    let mut stream = MarkdownStream::new();
    stream.push("```rust\nfn main");
    stream.push(&code["fn main".len()..]);
    assert_eq!(stream.open_code_block(), Some(0));
    let parses = thread_work().parses();
    let expected = analyze("rust", code).highlights;
    let streamed = &stream.document().code_blocks()[0].syntax().highlights;
    assert_eq!(thread_work().parses() - parses, 1, "only the reference is analyzed");
    for line in 0..3 {
        assert_eq!(streamed.line(line), expected.line(line));
    }
    let parses = thread_work().parses();
    stream.push("\n``");
    stream.push("`\n");
    assert_eq!(stream.open_code_block(), None);
    let closed = &stream.document().code_blocks()[0].syntax().highlights;
    assert_eq!(thread_work().parses(), parses, "closing reuses the streamed analysis");
    assert_eq!(closed.line_count(), 3);
    assert_eq!(stream.document().code_blocks()[0].text(), code);
    assert_eq!(closed.line(2).ok_or("line")?, expected.line(2).ok_or("line")?);
    Ok(())
}

#[test]
fn changes_report_the_first_changed_line_of_the_open_code_block() {
    let mut stream = MarkdownStream::new();
    stream.push("Intro\n\n```rust\nfn a() {}\n");
    let revision = stream.revision();
    stream.push("fn b() {}\n");
    let changes = stream.changes_since(revision);
    assert_eq!((changes.first_block, changes.first_line), (1, 1));
    stream.push("```\n\nAfter\n");
    assert_eq!(stream.changes_since(revision).first_block, 1);
}
