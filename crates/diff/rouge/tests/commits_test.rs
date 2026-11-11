mod common;

use clankerdiff_core::{
    DiffReviewEvent, DiffScope, InteractionPhase,
    testing::{DocumentBuilder, commit_id},
};
use clankerdiff_rouge::ReviewOutput;
use rouge_core::{Key, testing::TestResult};

#[test]
fn commit_picker_survives_compatible_refresh_and_chooses_with_the_keyboard() -> TestResult {
    let page = || {
        DocumentBuilder::new().commit("Add parser").commit("Wire parser").target(commit_id(2)).changed(
            "src/lib.rs",
            "old\n",
            "new\n",
        )
    };
    common::run(page().build(), |ui, handle, outputs| {
        ui.click_on("choose-commit")?;
        assert_eq!(handle.status().get().context.phase, InteractionPhase::CommitPicker);
        ui.center_of("commit-details")?;
        handle.set_document(page().build());
        ui.center_of("commit-picker")?;
        ui.press(Key::ArrowDown);
        ui.press(Key::Enter);
        assert_eq!(handle.status().get().context.phase, InteractionPhase::Browse);
        assert!(ui.bounds_of("commit-picker").is_none());
        assert!(outputs.borrow().contains(&ReviewOutput::Event(DiffReviewEvent::SetTarget(DiffScope::Both.into()))));

        ui.click_on("choose-commit")?;
        handle.set_document(page().commit("Ship parser").build());
        assert!(ui.bounds_of("commit-picker").is_none());
        assert_eq!(handle.status().get().context.phase, InteractionPhase::Browse);
        assert_eq!(outputs.borrow().len(), 1);
        Ok(())
    })
}
