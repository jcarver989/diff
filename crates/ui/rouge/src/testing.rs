pub use rouge_core::testing::{TestError, TestResult, TestUi, TestUiBuilder};
pub use rouge_reactive::Runtime;

#[must_use]
pub fn component_test() -> TestUiBuilder {
    TestUiBuilder::default().fonts(crate::fonts::bundled_fonts())
}
