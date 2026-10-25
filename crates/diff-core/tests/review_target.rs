use clankerdiff_core::{
    CommitId, DiffScope, ReviewTarget,
    testing::{DocumentBuilder, commit_id},
};
use std::error::Error;

#[test]
fn commit_ids_require_full_hexadecimal_names() -> Result<(), Box<dyn Error>> {
    let id = CommitId::new("ABCDEF0123456789abcdef0123456789abcdef01")?;
    assert_eq!(id.to_string(), "abcdef0123456789abcdef0123456789abcdef01");
    assert_eq!(id.short(), "abcdef0");
    for invalid in ["", "abc123", "--output=/tmp/x", "HEAD~1", &"g".repeat(40)] {
        assert!(
            CommitId::new(invalid).is_err(),
            "{invalid} should be rejected"
        );
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
    for target in [
        ReviewTarget::from(DiffScope::Staged),
        ReviewTarget::from(id.clone()),
    ] {
        let json = serde_json::to_string(&target)?;
        assert_eq!(serde_json::from_str::<ReviewTarget>(&json)?, target);
    }
    assert_eq!(
        ReviewTarget::default(),
        ReviewTarget::WorkingTree(DiffScope::Both)
    );
    assert_eq!(ReviewTarget::from(id.clone()).commit(), Some(&id));
    assert_eq!(ReviewTarget::from(id).scope(), None);
    Ok(())
}

#[test]
fn titles_number_pages_within_the_stack() {
    let document = DocumentBuilder::new().commit("Add parser").build();
    let stack = document.commits.as_deref();
    assert_eq!(
        ReviewTarget::Commit(commit_id(1)).title(stack),
        "1/2 · Commit 0000001 · Add parser"
    );
    assert_eq!(
        ReviewTarget::WorkingTree(DiffScope::Both).title(stack),
        "2/2 · Uncommitted changes (both)"
    );
    assert_eq!(
        ReviewTarget::Commit(commit_id(9)).title(stack),
        "Commit 0000009"
    );
    assert_eq!(
        ReviewTarget::Commit(commit_id(9)).to_string(),
        "commit 0000009"
    );
}
