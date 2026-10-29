use clankerdiff_gpui_ui::{
    prelude::Waveform,
    testing::{ComponentHarness, ComponentHarnessBuilder},
};
use gpui::{IntoElement, TestAppContext, WindowVisibility, black};
use std::time::Duration;

#[gpui::test]
fn a_waveform_redraws_at_most_thirty_times_a_second(cx: &mut TestAppContext) {
    let harness = waveform(cx);

    let redraws = harness.renders_over(cx, Duration::from_secs(1));

    assert!((10..=30).contains(&redraws), "redrew {redraws} times in a second");
}

#[gpui::test]
fn a_waveform_stops_redrawing_while_its_window_is_hidden(cx: &mut TestAppContext) {
    let harness = waveform(cx);
    assert!(harness.renders_over(cx, Duration::from_secs(1)) > 0);

    cx.simulate_window_visibility_change(harness.window(), WindowVisibility::Hidden);
    harness.renders_over(cx, Duration::from_secs(1));

    assert_eq!(harness.renders_over(cx, Duration::from_secs(1)), 0);
}

fn waveform(cx: &mut TestAppContext) -> ComponentHarness {
    ComponentHarnessBuilder::default().build(cx, |_, _| Waveform::new("waveform", black()).into_any_element())
}
