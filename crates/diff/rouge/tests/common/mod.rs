use clankerdiff_core::{
    DiffDocument, DiffReviewCommand, RepoPath,
    testing::{DocumentBuilder, location},
};
use clankerdiff_rouge::{ReviewController, ReviewHandle, ReviewOutput, diff_view};
use rouge_core::testing::{TestError, TestResult, TestUi, recorder};
use rouge_reactive::testing::Driver;
use rouge_widgets::testing::component_test;
use std::{
    cell::{Ref, RefCell},
    rc::Rc,
    sync::Arc,
};

pub struct ReviewHarness {
    pub ui: TestUi,
    pub handle: ReviewHandle,
    outputs: Rc<RefCell<Vec<ReviewOutput>>>,
    _driver: Driver,
}

#[allow(dead_code)]
impl ReviewHarness {
    pub fn new(document: Arc<DiffDocument>) -> Self {
        let driver = Driver::new();
        let (outputs, record) = recorder();
        let (mut ui, handle) = component_test().size(1000.0, 800.0).build_with(|| {
            let handle = ReviewController::new(document).mount(record);
            (diff_view(handle), handle)
        });
        assert!(
            driver.run_until(|| handle.with(ReviewController::symbols_ready)).is_ok(),
            "symbols never became ready"
        );
        ui.advance(0.0);
        Self { ui, handle, outputs, _driver: driver }
    }

    #[track_caller]
    pub fn select_file(&self, path: &str) -> TestResult {
        let path = RepoPath::new(path)?;
        let file = self.handle.with(|state| state.session().document().file_index(&path));
        let file = file.ok_or_else(|| format!("{path} is not in the diff"))?;
        assert!(self.handle.handle_command(DiffReviewCommand::SelectFile(file)), "could not select {path}");
        Ok(())
    }

    #[track_caller]
    pub fn press(&mut self, key: &str) {
        assert!(self.ui.press(key), "key {key:?} was not handled");
    }

    pub fn click(&mut self, id: &str) -> Result<(), TestError> {
        self.ui.click_on(id)
    }

    #[track_caller]
    pub fn assert_visible(&mut self, id: &str) {
        assert!(self.ui.rect_of(id).is_ok(), "expected {id:?} to be visible");
    }

    #[track_caller]
    pub fn assert_hidden(&mut self, id: &str) {
        assert!(self.ui.rect_of(id).is_err(), "expected {id:?} to be hidden");
    }

    #[track_caller]
    pub fn assert_selected_line(&self, path: &str, line: usize) -> TestResult {
        let selected = self.handle.with(|state| state.session().selected_source_line());
        assert_eq!(selected, Some(location(path, line)?));
        Ok(())
    }

    pub fn outputs(&self) -> Ref<'_, [ReviewOutput]> {
        Ref::map(self.outputs.borrow(), Vec::as_slice)
    }
}

#[allow(dead_code)]
pub fn document() -> Arc<DiffDocument> {
    DocumentBuilder::new().changed("src/lib.rs", "old\n", "new\n").changed("src/main.rs", "", "fn main() {}\n").build()
}
