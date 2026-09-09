#![cfg(feature = "test-support")]

use clankerdiff_core::{ReviewSession, testing::DocumentBuilder};
use std::error::Error;

#[test]
fn edit_and_undo_preserve_comment_identity() -> Result<(), Box<dyn Error>> {
    let mut session = session();
    assert!(session.begin_draft(None));
    session
        .draft_mut()
        .ok_or("missing draft")?
        .set_body("First comment");
    let id = session.submit_draft().ok_or("missing comment")?;
    assert!(session.edit_comment_at_selection());
    assert_eq!(
        session.draft().ok_or("missing edit draft")?.body(),
        "First comment"
    );
    session
        .draft_mut()
        .ok_or("missing draft")?
        .set_body("Updated comment");
    assert_eq!(session.submit_draft(), Some(id));
    assert_eq!(session.review().len(), 1);
    assert!(session.undo_last_comment());
    assert!(session.review().is_empty());
    assert!(!session.undo_last_comment());
    Ok(())
}

#[test]
fn missing_comment_operations_do_not_create_drafts() {
    let mut session = session();
    assert!(!session.edit_comment_at_selection());
    assert!(!session.delete_comment_at_selection());
    assert!(!session.undo_last_comment());
    assert!(session.draft().is_none());
    assert!(session.review().is_empty());
    let mut empty = ReviewSession::new(DocumentBuilder::new().build());
    assert!(!empty.begin_draft(None));
}

#[test]
fn blank_submission_closes_the_draft_without_adding_a_comment() {
    let mut session = session();
    assert!(session.begin_draft(None));
    assert_eq!(session.submit_draft(), None);
    assert!(session.draft().is_none());
    assert!(session.review().is_empty());
}

fn session() -> ReviewSession {
    ReviewSession::new(
        DocumentBuilder::new()
            .changed("a.rs", "old\n", "new\n")
            .build(),
    )
}
