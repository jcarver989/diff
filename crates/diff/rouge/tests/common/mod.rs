use clankerdiff_core::{DiffDocument, testing::DocumentBuilder};
use clankerdiff_rouge::{ReviewController, ReviewHandle, ReviewOutput, diff_view};
use rouge_core::testing::{TestUi, TestUiBuilder};
use rouge_reactive::Runtime;
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub fn run<T>(
    document: Arc<DiffDocument>,
    test: impl FnOnce(&mut TestUi, ReviewHandle, &RefCell<Vec<ReviewOutput>>) -> T,
) -> T {
    let _runtime = Runtime::new().enter();
    let outputs = Rc::new(RefCell::new(Vec::new()));
    let (mut ui, handle) = TestUiBuilder::default()
        .fonts(clankerdiff_rouge::ui::fonts::bundled_fonts())
        .size(1000.0, 800.0)
        .build_with(|| {
            let sink = outputs.clone();
            let handle = ReviewController::new(document).mount(move |event| sink.borrow_mut().push(event));
            (diff_view(handle.clone()), handle)
        });
    test(&mut ui, handle, &outputs)
}

#[allow(dead_code)]
pub fn document() -> Arc<DiffDocument> {
    DocumentBuilder::new().changed("src/lib.rs", "old\n", "new\n").changed("src/main.rs", "", "fn main() {}\n").build()
}
