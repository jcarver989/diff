mod common;

use clankerdiff_core::{
    DiffReviewEvent, DiffScope, InteractionPhase,
    testing::{DocumentBuilder, commit_id},
};
use clankerdiff_rouge::ReviewOutput;
use common::ReviewHarness;
use rouge_core::testing::TestResult;

#[test]
fn commit_picker_survives_compatible_refresh_and_chooses_with_the_keyboard() -> TestResult {
    let page = || {
        DocumentBuilder::new().commit("Add parser").commit("Wire parser").target(commit_id(2)).changed(
            "src/lib.rs",
            "old\n",
            "new\n",
        )
    };
    let mut review = ReviewHarness::new(page().build());
    review.click("choose-commit")?;
    assert_eq!(review.handle.status().get().context.phase, InteractionPhase::CommitPicker);
    review.assert_visible("commit-details");
    review.handle.set_document(page().build());
    review.assert_visible("commit-picker");

    review.press("down");
    review.press("enter");
    assert_eq!(review.handle.status().get().context.phase, InteractionPhase::Browse);
    review.assert_hidden("commit-picker");
    assert!(review.outputs().contains(&ReviewOutput::Event(DiffReviewEvent::SetTarget(DiffScope::Both.into()))));

    review.click("choose-commit")?;
    review.handle.set_document(page().commit("Ship parser").build());
    review.assert_hidden("commit-picker");
    assert_eq!(review.handle.status().get().context.phase, InteractionPhase::Browse);
    assert_eq!(review.outputs().len(), 1);
    Ok(())
}
