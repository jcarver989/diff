mod common;

use clankerdiff_core::{DiffReviewEvent, InteractionPhase, ReviewCommand, testing::DocumentBuilder};
use clankerdiff_rouge::ReviewOutput;
use common::ReviewHarness;
use rouge_core::{Point, testing::TestResult};
use std::io;

#[test]
fn inline_editor_grows_and_publishes_edit_delete_and_export() -> TestResult {
    let mut review = ReviewHarness::new(common::document());
    let row = review.handle.selection().get().row.ok_or_else(|| io::Error::other("no selected row"))?;
    let side = review.handle.selection().get().side;
    review.ui.hover_on(&format!("add-comment-{row}-{side:?}"))?;
    review.ui.advance(0.0);
    review.click(&format!("add-comment-{row}-{side:?}"))?;
    let short = review.ui.bounds_of("comment-composer").ok_or_else(|| io::Error::other("no inline editor"))?;
    let code = review.ui.center_of(&format!("diff-cell-{row}-{side:?}"))?;
    assert!(short.origin.y > code.y);
    for _ in 0..8 {
        review.ui.type_text("comment");
        review.press("shift-enter");
    }
    let tall = review.ui.bounds_of("comment-composer").ok_or_else(|| io::Error::other("editor disappeared"))?;
    assert!(tall.size.height > short.size.height);
    review.press("enter");
    let id = review
        .handle
        .with(|s| s.session().last_comment_id())
        .ok_or_else(|| io::Error::other("no submitted comment"))?;
    review.assert_visible(&format!("comment-{id}"));
    review.click(&format!("Edit-comment-{id}"))?;
    assert_eq!(review.handle.status().get().context.phase, InteractionPhase::Draft);
    review.press("cmd-a");
    review.ui.type_text("replacement");
    review.press("enter");
    review.click("copy-review")?;
    assert!(review.outputs().iter().any(|event| matches!(event, ReviewOutput::Event(DiffReviewEvent::CopyFormattedReview(text)) if text.contains("replacement"))));
    review.click(&format!("Delete-comment-{id}"))?;
    assert!(review.handle.with(|s| s.session().review().is_empty()));
    assert!(review.handle.handle_command(ReviewCommand::BeginComment));
    review.ui.type_text("   ");
    review.press("enter");
    assert!(review.handle.with(|s| s.session().review().is_empty()));
    Ok(())
}

#[test]
fn editing_one_comment_replaces_its_body_in_place_and_can_be_cancelled() -> TestResult {
    let mut review = ReviewHarness::new(common::document());
    let mut ids = Vec::new();
    for body in ["first comment", "second comment"] {
        assert!(review.handle.handle_command(ReviewCommand::BeginComment));
        review.ui.type_text(body);
        review.press("enter");
        ids.push(review.handle.with(|s| s.session().last_comment_id()).ok_or_else(|| io::Error::other("no comment"))?);
    }
    let [first, second] = ids.as_slice() else { return Err(io::Error::other("expected two comments").into()) };
    let card_id = format!("comment-{first}");
    let original = review.ui.rect_of(&card_id)?;
    review.click(&format!("Edit-comment-{first}"))?;
    let card = review.ui.rect_of(&card_id)?;
    let editor = review.ui.rect_of("comment-editor")?;
    assert_eq!(card.origin, original.origin);
    assert!(card.contains(editor.origin) && card.contains(editor.max()), "{editor:?} outside {card:?}");
    assert_eq!(review.ui.bounds_of_all("comment-editor").len(), 1);
    review.assert_visible(&format!("Edit-comment-{second}"));
    assert_eq!(
        review.handle.with(|s| s.session().draft().map(|d| d.body().to_owned())).as_deref(),
        Some("first comment")
    );

    review.press("cmd-a");
    review.ui.type_text("discard this edit");
    review.click("cancel-comment")?;
    review.assert_hidden("comment-editor");
    assert!(review.ui.text_bounds("first comment").is_some());
    assert!(review.ui.text_bounds("second comment").is_some());
    assert!(review.ui.text_bounds("discard this edit").is_none());
    Ok(())
}

#[test]
fn compatible_refresh_preserves_focused_draft_selection_and_undo() {
    let mut review = ReviewHarness::new(common::document());
    assert!(review.handle.handle_command(ReviewCommand::BeginComment));
    review.ui.type_text("x");
    review.press("shift-left");
    review.handle.set_document(common::document());
    review.ui.advance(0.0);
    review.ui.type_text("y");
    let handle = review.handle;
    let body = || handle.with(|s| s.session().draft().map(|draft| draft.body().to_owned()));
    assert_eq!(body().as_deref(), Some("y"));
    for expected in ["x", ""] {
        review.press("cmd-z");
        assert_eq!(body().as_deref(), Some(expected));
    }
    for expected in ["x", "y"] {
        review.press("cmd-shift-z");
        assert_eq!(body().as_deref(), Some(expected));
    }
    review.press("enter");
    assert!(review.handle.with(|s| s.session().review().comments().any(|c| c.body == "y")));
}

#[test]
fn scrolling_a_draft_offscreen_preserves_the_anchor_and_body() -> TestResult {
    let lines = "new line\n".repeat(200);
    let doc = DocumentBuilder::new().changed("a.txt", "", &lines).changed("b.txt", "", &lines).build();
    let mut review = ReviewHarness::new(doc);
    assert!(review.handle.handle_command(ReviewCommand::BeginComment));
    review.ui.type_text("draft");
    let selection = review.handle.selection().get();
    let handle = review.handle;
    let draft = || handle.with(|s| s.session().draft().map(|d| (d.anchor().clone(), d.body().to_owned())));
    let initial = draft().ok_or_else(|| io::Error::other("no draft"))?;
    let point = review.ui.center_of("diff-pane")?;
    review.ui.pointer_moved(point);
    review.ui.scroll(Point::new(0.0, -5000.0));
    review.ui.advance(0.0);
    review.assert_hidden("comment-editor");
    assert_eq!(review.handle.selection().get().row, selection.row);
    assert_eq!(review.handle.selection().get().file, selection.file);
    assert_eq!(draft(), Some(initial));
    review.ui.scroll(Point::new(0.0, 5000.0));
    review.ui.advance(0.0);
    review.click("comment-editor")?;
    review.press("enter");
    assert!(review.handle.with(|s| s.session().review().comments().any(|c| c.body == "draft")));
    Ok(())
}
