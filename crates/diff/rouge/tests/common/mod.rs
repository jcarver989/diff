use clankerdiff_core::{DiffDocument, testing::DocumentBuilder};
use clankerdiff_rouge::{ReviewController, ReviewHandle, ReviewOutput, diff_view};
use rouge_core::testing::TestUi;
use rouge_reactive::testing::Driver;
use rouge_widgets::testing::component_test;
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub fn run<T>(
    document: Arc<DiffDocument>,
    test: impl FnOnce(&mut TestUi, ReviewHandle, &RefCell<Vec<ReviewOutput>>) -> T,
) -> T {
    let driver = Driver::new();
    let outputs = Rc::new(RefCell::new(Vec::new()));
    let (mut ui, handle) = component_test().size(1000.0, 800.0).build_with(|| {
        let sink = outputs.clone();
        let handle = ReviewController::new(document).mount(move |event| sink.borrow_mut().push(event));
        (diff_view(handle), handle)
    });
    assert!(driver.run_until(|| handle.with(ReviewController::symbols_ready)).is_ok(), "symbols never became ready");
    ui.advance(0.0);
    test(&mut ui, handle, &outputs)
}

#[allow(dead_code)]
pub fn document() -> Arc<DiffDocument> {
    DocumentBuilder::new().changed("src/lib.rs", "old\n", "new\n").changed("src/main.rs", "", "fn main() {}\n").build()
}
