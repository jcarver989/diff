use clankerdiff_core::{
    DiffError, DiffSide, FileDiff, FileStatus, PatchLineKind, RepoPath, SourceDocument, SourceUnavailable, StageState,
    parse_git_diff,
};
use std::{error::Error, fmt::Write, sync::Arc};

#[test]
fn builds_canonical_numbered_diff() -> Result<(), DiffError> {
    let diff = FileDiff::from_texts("main.rs", "keep\nold", "keep\nnew")?;
    assert_eq!(diff.status, FileStatus::Modified);
    assert_eq!((diff.additions(), diff.deletions()), (1, 1));
    assert!(diff.no_newline_at_end);
    assert_eq!(diff.hunks[0].lines[0].kind, PatchLineKind::Context);
    assert_eq!(diff.hunks[0].lines[2].new_line_no, Some(2));
    Ok(())
}

#[test]
fn groups_distant_changes_into_context_limited_hunks() -> Result<(), Box<dyn Error>> {
    let mut old = String::new();
    for line in 1..=20 {
        writeln!(old, "line {line}")?;
    }
    let new = old.replace("line 2\n", "changed 2\n").replace("line 19\n", "changed 19\n");
    let diff = FileDiff::from_texts("main.rs", &old, &new)?;
    assert_eq!(diff.hunks.len(), 2);
    assert!(diff.hunks.iter().all(|hunk| hunk.lines.len() <= 8));
    Ok(())
}

#[test]
fn strips_carriage_returns_like_str_lines() -> Result<(), DiffError> {
    let diff = FileDiff::from_texts("main.rs", "a\r\n", "b\r\n")?;
    assert_eq!(diff.hunks[0].lines[0].text.as_ref(), "a");
    assert_eq!(diff.hunks[0].lines[1].text.as_ref(), "b");
    Ok(())
}

#[test]
fn from_texts_records_absent_sides_and_captures_the_rest() -> Result<(), DiffError> {
    let added = FileDiff::from_texts("a.rs", "", "new\n")?;
    assert_eq!(added.source_unavailable(DiffSide::Old), Some(&SourceUnavailable::Absent));
    assert_eq!(added.source_document(DiffSide::New).map(|source| source.text()), Some("new\n"));
    let deleted = FileDiff::from_texts("a.rs", "old\n", "")?;
    assert_eq!(deleted.source_unavailable(DiffSide::New), Some(&SourceUnavailable::Absent));
    Ok(())
}

#[test]
fn attaching_sources_rederives_hunks_but_keeps_git_metadata() -> Result<(), Box<dyn Error>> {
    let mut patch = FileDiff::from_texts("a.rs", "stale\n", "patch\n")?;
    patch.status = FileStatus::Renamed;
    patch.staged = StageState::Staged;
    patch.old_path = Some(RepoPath::new("b.rs")?);
    patch.omitted_bytes = Some(7);
    let old = Arc::new(SourceDocument::new("keep\nold")?);
    let new = Arc::new(SourceDocument::new("keep\nnew")?);
    let rebuilt = patch.clone().with_sources(Ok(old.clone()), Ok(new));
    assert_eq!(rebuilt.status, FileStatus::Renamed);
    assert_eq!(rebuilt.staged, StageState::Staged);
    assert_eq!(rebuilt.old_path.as_ref().map(RepoPath::as_str), Some("b.rs"));
    assert_eq!(rebuilt.omitted_bytes, Some(7));
    assert!(rebuilt.no_newline_at_end);
    assert_eq!(rebuilt.hunks[0].lines[0].text.as_ref(), "keep");
    assert_eq!((rebuilt.additions(), rebuilt.deletions()), (1, 1));

    let partial = patch.clone().with_sources(Ok(old), Err(SourceUnavailable::TooLarge { bytes: 1 }));
    assert_eq!(partial.hunks, patch.hunks);

    let mut binary = patch.clone();
    binary.binary = true;
    let binary = binary.with_sources(Ok(Arc::new(SourceDocument::new("a")?)), Ok(Arc::new(SourceDocument::new("b")?)));
    assert_eq!(binary.hunks, patch.hunks);
    Ok(())
}

#[test]
fn content_id_tracks_sides_and_only_hunks_when_a_side_is_unavailable() -> Result<(), DiffError> {
    let file = FileDiff::from_texts("a.rs", "one\n", "two\n")?;
    let same = FileDiff { staged: StageState::Staged, old_path: Some(RepoPath::new("moved.rs")?), ..file.clone() };
    assert_eq!(file.content_id(), same.content_id());
    assert_ne!(file.content_id(), FileDiff::from_texts("a.rs", "one\n", "three\n")?.content_id());

    let patch_only = FileDiff { new_source: Err(SourceUnavailable::TooLarge { bytes: 9 }), ..file.clone() };
    let edited_patch = FileDiff { hunks: Vec::new(), ..patch_only.clone() };
    assert_ne!(patch_only.content_id(), edited_patch.content_id());
    assert_ne!(patch_only.content_id(), file.content_id());
    Ok(())
}

#[test]
fn renamed_from_ignores_an_unchanged_old_path() -> Result<(), Box<dyn Error>> {
    let mut file = FileDiff::from_texts("new.rs", "a\n", "b\n")?;
    assert_eq!(file.renamed_from(), None);
    file.old_path = Some(RepoPath::new("new.rs")?);
    assert_eq!(file.renamed_from(), None);
    file.old_path = Some(RepoPath::new("old.rs")?);
    assert_eq!(file.renamed_from().map(RepoPath::as_str), Some("old.rs"));
    Ok(())
}

#[test]
fn unchanged_old_lines_map_through_the_hunks_to_their_new_line() -> Result<(), Box<dyn Error>> {
    let mut old = String::new();
    for line in 1..=20 {
        writeln!(old, "line {line}")?;
    }
    let new = old.replace("line 2\n", "added\nchanged 2\n").replace("line 19\n", "");
    let diff = FileDiff::from_texts("main.rs", &old, &new)?;
    let mapped: Vec<_> = [1, 2, 3, 10, 18, 19, 20, 21].map(|line| diff.unchanged_new_line(line)).to_vec();
    assert_eq!(mapped, [Some(1), None, Some(4), Some(11), Some(19), None, Some(20), Some(21)]);
    Ok(())
}

#[test]
fn zero_context_hunks_map_lines_on_either_side_of_an_insertion() -> Result<(), Box<dyn Error>> {
    let patch = "diff --git a/main.rs b/main.rs\n--- a/main.rs\n+++ b/main.rs\n@@ -3,0 +4,2 @@\n+one\n+two\n";
    let files = parse_git_diff(patch.as_bytes())?;
    let diff = files.first().ok_or("a parsed file")?;
    assert_eq!([3, 4].map(|line| diff.unchanged_new_line(line)), [Some(3), Some(6)]);
    Ok(())
}
