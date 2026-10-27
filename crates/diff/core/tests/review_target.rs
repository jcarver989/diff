use clankerdiff_core::{
    CommitId, DiffScope, PageKind, ReviewTarget, UNCOMMITTED_CHANGES,
    testing::{DocumentBuilder, commit_id},
};
use std::error::Error;

#[test]
fn commit_ids_require_full_hexadecimal_names() -> Result<(), Box<dyn Error>> {
    let id = CommitId::new("ABCDEF0123456789abcdef0123456789abcdef01")?;
    assert_eq!(id.to_string(), "abcdef0123456789abcdef0123456789abcdef01");
    assert_eq!(id.short(), "abcdef0");
    for invalid in ["", "abc123", "--output=/tmp/x", "HEAD~1", &"g".repeat(40)] {
        assert!(CommitId::new(invalid).is_err(), "{invalid} should be rejected");
    }
    Ok(())
}

#[test]
fn deserializing_rejects_invalid_commit_ids() {
    let json = r#"{"commit":"--upload-pack=evil"}"#;
    assert!(serde_json::from_str::<ReviewTarget>(json).is_err());
}

#[test]
fn targets_round_trip_and_expose_their_parts() -> Result<(), Box<dyn Error>> {
    let id = CommitId::new("a".repeat(40))?;
    for target in [ReviewTarget::from(DiffScope::Staged), ReviewTarget::from(id.clone())] {
        let json = serde_json::to_string(&target)?;
        assert_eq!(serde_json::from_str::<ReviewTarget>(&json)?, target);
    }
    assert_eq!(ReviewTarget::default(), ReviewTarget::WorkingTree(DiffScope::Both));
    assert_eq!(ReviewTarget::from(id.clone()).commit(), Some(&id));
    assert_eq!(ReviewTarget::from(id).scope(), None);
    Ok(())
}

#[test]
fn labels_number_pages_within_the_stack() {
    let document = DocumentBuilder::new().commit("Add parser").build();
    let stack = document.commits.as_deref();
    let commit = ReviewTarget::Commit(commit_id(1)).label(stack);
    assert_eq!(commit.position, Some((1, 2)));
    assert_eq!(
        commit.kind,
        PageKind::Commit { short_id: "0000001".to_owned(), subject: Some("Add parser".to_owned()) }
    );
    assert_eq!(commit.to_string(), "1/2 · Commit 0000001 · Add parser");
    assert_eq!(ReviewTarget::WorkingTree(DiffScope::Both).label(stack).to_string(), "2/2 · Uncommitted changes (both)");
    let unknown = ReviewTarget::Commit(commit_id(9)).label(stack);
    assert_eq!(unknown.position, None);
    assert_eq!(unknown.heading(), "Commit 0000009");
    assert_eq!(ReviewTarget::Commit(commit_id(9)).to_string(), "commit 0000009");
}

#[test]
fn labels_expose_short_ids_subjects_and_positions() {
    let document = DocumentBuilder::new().commit("Add parser").build();
    let stack = document.commits.as_deref();
    let commit = ReviewTarget::Commit(commit_id(1)).label(stack);
    assert_eq!(commit.short_id(), Some("0000001"));
    assert_eq!(commit.subject(), Some("Add parser"));
    assert_eq!(commit.position_label().as_deref(), Some("1/2"));
    assert!(commit.is_paged());

    let working_tree = ReviewTarget::WorkingTree(DiffScope::Staged).label(stack);
    assert_eq!(working_tree.short_id(), None);
    assert_eq!(working_tree.subject(), Some(UNCOMMITTED_CHANGES));

    let unknown = ReviewTarget::Commit(commit_id(9)).label(stack);
    assert_eq!((unknown.short_id(), unknown.subject(), unknown.position_label()), (Some("0000009"), None, None));
    assert!(unknown.is_paged());
}

#[test]
fn a_stack_without_commits_is_a_single_unpaged_page() {
    let document = DocumentBuilder::new().commits_since("origin/main").build();
    let label = ReviewTarget::WorkingTree(DiffScope::Both).label(document.commits.as_deref());
    assert_eq!(label.position, Some((1, 1)));
    assert!(!label.is_paged());
}
