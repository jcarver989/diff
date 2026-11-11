use clankerdiff_core::testing::DocumentBuilder;
use clankerdiff_rouge::{ReviewController, diff_view};
use clankerdiff_rouge_ui::{components::Button, fonts::bundled_fonts, theme::UiTheme};
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{Styled, div, testing::TestUiBuilder};
use rouge_reactive::Runtime;

#[test]
fn an_external_crate_can_mount_and_render_review_and_ui() -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = Runtime::new().enter();
    let mut ui = TestUiBuilder::default().fonts(bundled_fonts()).size(1000.0, 700.0).build(|| {
        let document = DocumentBuilder::new().changed("lib.rs", "old\n", "new\n").build();
        let review = ReviewController::new(document).mount(|_| {});
        let theme = UiTheme::new(&ReviewTheme::default());
        div().size_full().flex().col().child(Button::new("fixture", "Fixture", theme)).child(diff_view(review))
    });
    ui.center_of("fixture")?;
    assert_eq!(ui.render()?.width, 1000);
    Ok(())
}
