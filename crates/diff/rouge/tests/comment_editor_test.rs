mod common;

use clankerdiff_core::{DiffReviewEvent, InteractionPhase, ReviewCommand, testing::DocumentBuilder};
use clankerdiff_rouge::ReviewOutput;
use rouge_core::{Point, testing::TestResult};
use std::io;

#[test]
fn inline_editor_grows_and_publishes_edit_delete_and_export() -> TestResult {
    common::run(common::document(), |ui, handle, outputs| {
        let row = handle.selection().get().row.ok_or_else(|| io::Error::other("no selected row"))?;
        let side = handle.selection().get().side;
        ui.hover_on(&format!("add-comment-{row}-{side:?}"))?;
        ui.advance(0.0);
        ui.click_on(&format!("add-comment-{row}-{side:?}"))?;
        let short = ui.bounds_of("comment-composer").ok_or_else(|| io::Error::other("no inline editor"))?;
        let code = ui.center_of(&format!("diff-cell-{row}-{side:?}"))?;
        assert!(short.origin.y > code.y);
        for _ in 0..8 {
            ui.type_text("comment");
            ui.press("shift-enter");
        }
        let tall = ui.bounds_of("comment-composer").ok_or_else(|| io::Error::other("editor disappeared"))?;
        assert!(tall.size.height > short.size.height);
        ui.press("enter");
        let id =
            handle.with(|s| s.session().last_comment_id()).ok_or_else(|| io::Error::other("no submitted comment"))?;
        assert!(ui.bounds_of(&format!("comment-{id}")).is_some());
        ui.click_on(&format!("Edit-comment-{id}"))?;
        assert_eq!(handle.status().get().context.phase, InteractionPhase::Draft);
        ui.press("cmd-a");
        ui.type_text("replacement");
        ui.press("enter");
        ui.click_on("copy-review")?;
        assert!(outputs.borrow().iter().any(|event| matches!(event, ReviewOutput::Event(DiffReviewEvent::CopyFormattedReview(text)) if text.contains("replacement"))));
        ui.click_on(&format!("Delete-comment-{id}"))?;
        assert!(handle.with(|s| s.session().review().is_empty()));
        assert!(handle.handle_command(ReviewCommand::BeginComment));
        ui.type_text("   ");
        ui.press("enter");
        assert!(handle.with(|s| s.session().review().is_empty()));
        Ok(())
    })
}

#[test]
fn compatible_refresh_preserves_focused_draft_selection_and_undo() -> TestResult {
    common::run(common::document(), |ui, handle, _| {
        assert!(handle.handle_command(ReviewCommand::BeginComment));
        ui.type_text("x");
        ui.press("shift-left");
        handle.set_document(common::document());
        ui.advance(0.0);
        ui.type_text("y");
        let body = || handle.with(|s| s.session().draft().map(|draft| draft.body().to_owned()));
        assert_eq!(body().as_deref(), Some("y"));
        for expected in ["x", ""] {
            ui.press("cmd-z");
            assert_eq!(body().as_deref(), Some(expected));
        }
        for expected in ["x", "y"] {
            ui.press("cmd-shift-z");
            assert_eq!(body().as_deref(), Some(expected));
        }
        ui.press("enter");
        assert!(handle.with(|s| s.session().review().comments().any(|c| c.body == "y")));
        Ok(())
    })
}

#[test]
fn scrolling_a_draft_offscreen_preserves_the_anchor_and_body() -> TestResult {
    let lines = "new line\n".repeat(200);
    let doc = DocumentBuilder::new().changed("a.txt", "", &lines).changed("b.txt", "", &lines).build();
    common::run(doc, |ui, handle, _| {
        assert!(handle.handle_command(ReviewCommand::BeginComment));
        ui.type_text("draft");
        let selection = handle.selection().get();
        let draft = || handle.with(|s| s.session().draft().map(|d| (d.anchor().clone(), d.body().to_owned())));
        let initial = draft().ok_or_else(|| io::Error::other("no draft"))?;
        let point = ui.center_of("diff-pane")?;
        ui.pointer_moved(point);
        ui.scroll(Point::new(0.0, -5000.0));
        ui.advance(0.0);
        assert!(ui.bounds_of("comment-editor").is_none());
        assert_eq!(handle.selection().get().row, selection.row);
        assert_eq!(handle.selection().get().file, selection.file);
        assert_eq!(draft(), Some(initial));
        ui.scroll(Point::new(0.0, 5000.0));
        ui.advance(0.0);
        ui.click_on("comment-editor")?;
        ui.press("enter");
        assert!(handle.with(|s| s.session().review().comments().any(|c| c.body == "draft")));
        Ok(())
    })
}
