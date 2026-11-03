#![cfg(feature = "review")]

use clankerdiff_markdown::{MarkdownDocument, MarkdownReviewDecision, MarkdownReviewEvent, MarkdownReviewSession};
use std::{error::Error, sync::Arc};

#[test]
fn edit_cancel_and_undo_preserve_review_comments() -> Result<(), Box<dyn Error>> {
    let mut session = session();
    assert!(session.begin_draft(None));
    session.draft_mut().ok_or("missing draft")?.set_body("Please clarify");
    assert!(session.submit_draft().is_some());
    assert_eq!(session.review().len(), 1);
    let MarkdownReviewEvent::Submit(submission) = session.request_changes()? else {
        return Err("missing submission".into());
    };
    assert_eq!(submission.decision, MarkdownReviewDecision::ChangesRequested);
    assert!(session.edit_comment_at_selection());
    assert_eq!(session.draft().ok_or("missing edit draft")?.body(), "Please clarify");
    session.cancel_draft();
    assert_eq!(session.review().len(), 1);
    assert!(session.undo_last_comment());
    assert!(session.review().is_empty());
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
    let mut empty = MarkdownReviewSession::new(Arc::new(MarkdownDocument::parse("")));
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

#[test]
fn annotations_revision_advances_only_when_comments_or_the_draft_change() -> Result<(), Box<dyn Error>> {
    let mut session = session();
    let initial = session.annotations_revision();
    session.cancel_draft();
    assert_eq!(session.annotations_revision(), initial);
    assert!(session.begin_draft(None));
    session.draft_mut().ok_or("missing draft")?.set_body("Please clarify");
    session.submit_draft().ok_or("missing comment")?;
    let submitted = session.annotations_revision();
    assert_ne!(submitted, initial);
    session.replace_document(Arc::new(MarkdownDocument::parse(SOURCE)));
    assert_eq!(session.annotations_revision(), submitted);
    session.replace_document(Arc::new(MarkdownDocument::parse("Unrelated")));
    assert_ne!(session.annotations_revision(), submitted);
    Ok(())
}

const SOURCE: &str = "# One\n\nText\n\n## Two\n\nMore text";

fn session() -> MarkdownReviewSession {
    MarkdownReviewSession::new(Arc::new(MarkdownDocument::parse(SOURCE)))
}
