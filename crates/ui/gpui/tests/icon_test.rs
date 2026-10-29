use clankerdiff_gpui_ui::{
    prelude::{Icon, IconName, spin_when_shown},
    testing::{ComponentHarness, ComponentHarnessBuilder},
};
use gpui::{AppContext as _, IntoElement, TestAppContext, WindowVisibility, black};
use std::time::Duration;

#[gpui::test]
fn a_spinner_redraws_at_most_ten_times_a_second(cx: &mut TestAppContext) {
    let harness = spinner(cx);

    let redraws = harness.renders_over(cx, Duration::from_secs(1));

    assert!((1..=10).contains(&redraws), "redrew {redraws} times in a second");
}

#[gpui::test]
fn a_spinner_stops_redrawing_while_its_window_is_hidden(cx: &mut TestAppContext) {
    let harness = spinner(cx);
    assert!(harness.renders_over(cx, Duration::from_secs(1)) > 0);

    cx.simulate_window_visibility_change(harness.window(), WindowVisibility::Hidden);
    // The redraw scheduled before the window was hidden still lands, and draws it still.
    harness.renders_over(cx, Duration::from_secs(1));

    assert_eq!(harness.renders_over(cx, Duration::from_secs(1)), 0);
}

#[gpui::test]
fn a_spinner_resumes_once_its_window_is_shown_again(cx: &mut TestAppContext) {
    let harness = spinner(cx);
    cx.update_window(harness.window(), |_, window, _| {
        spin_when_shown(window).detach();
    })
    .unwrap();
    cx.simulate_window_visibility_change(harness.window(), WindowVisibility::Hidden);
    harness.renders_over(cx, Duration::from_secs(1));

    cx.simulate_window_visibility_change(harness.window(), WindowVisibility::Visible);
    harness.renders_over(cx, Duration::from_millis(100));

    assert!(harness.renders_over(cx, Duration::from_secs(1)) > 0);
}

/// A spinning icon.
fn spinner(cx: &mut TestAppContext) -> ComponentHarness {
    ComponentHarnessBuilder::default()
        .build(cx, |_, _| Icon::new(IconName::LoaderCircle, black()).spin("spinner").into_any_element())
}
