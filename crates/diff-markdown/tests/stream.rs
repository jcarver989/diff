use clankerdiff_markdown::MarkdownStream;

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
