use clankerdiff_rouge_ui::{
    prelude::*,
    testing::{TestResult, component_test},
};
use rouge_core::{Point, ScrollState, Scrollable, Styled, div, px, scroll};
use rouge_reactive::signal;

#[test]
fn overflow_shows_a_scaled_thumb_that_tracks_only_its_own_list() -> TestResult {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    let (mut ui, (state, other, size)) = component_test().size(360.0, 200.0).build_with(|| {
        let size = InterfaceSize::install(16.0);
        let state = signal(ScrollState::default());
        let other = signal(ScrollState::default());
        let panel = |state, height| {
            div()
                .relative()
                .w(px(160.0))
                .h(px(160.0))
                .child(scroll().size_full().scroll_state(state).child(div().h(px(height))))
                .child(ListScrollbar::new(state))
        };
        (div().flex().child(panel(state, 900.0)).child(panel(other, 60.0)), (state, other, size))
    });
    assert_eq!(ui.bounds_of_all("scroll-thumb-vertical").len(), 1);
    let thumb = ui.center_of("scroll-thumb-vertical")?;
    ui.click_at(thumb.x, 90.0);
    assert!(state.get().offset.y > 0.0);
    ui.drag_on("scroll-thumb-vertical", Point::new(thumb.x, 180.0))?;
    assert!((state.get().offset.y - 740.0).abs() < 1.0);
    assert!(other.get().offset.y.abs() < 0.01);
    size.set(24.0);
    ui.advance(0.0);
    assert!(ui.center_of("scroll-thumb-vertical")?.x < thumb.x);
    Ok(())
}
