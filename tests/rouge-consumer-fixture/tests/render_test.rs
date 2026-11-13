use clankerdiff_core::testing::DocumentBuilder;
use clankerdiff_rouge::{ReviewController, diff_view, theme::ActiveReviewTheme};
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{Styled, div, view};
use rouge_widgets::{button, testing::component_test};
use rouge_reactive::Runtime;

#[test]
fn an_external_crate_can_mount_and_render_review_and_ui() -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = Runtime::new().enter();
    let mut ui = component_test().size(1000.0, 700.0).build(|| {
        let document = DocumentBuilder::new().changed("lib.rs", "old\n", "new\n").build();
        let review = ReviewController::new(document).mount(|_| {});
        let _ = ActiveReviewTheme::install(ReviewTheme::default());
        view! {
            <div size_full flex col>
                <button id="fixture" label="Fixture"/>
                {diff_view(review)}
            </div>
        }
    });
    ui.center_of("fixture")?;
    assert_eq!(ui.render()?.width, 1000);
    Ok(())
}
