use clankerdiff_gpui_ui::{
    prelude::{Icon, IconName, spin_when_shown},
    testing::{ComponentHarness, ComponentHarnessBuilder},
};
use gpui::{AppContext as _, IntoElement, TestAppContext, WindowVisibility, black};
use std::{cell::Cell, rc::Rc, time::Duration};

#[gpui::test]
fn a_spinner_redraws_at_most_ten_times_a_second(cx: &mut TestAppContext) {
    let (_harness, renders) = spinner(cx);

    let redraws = redraws_over(cx, &renders, Duration::from_secs(1));

    assert!(
        (1..=10).contains(&redraws),
        "redrew {redraws} times in a second"
    );
}

#[gpui::test]
fn a_spinner_stops_redrawing_while_its_window_is_hidden(cx: &mut TestAppContext) {
    let (harness, renders) = spinner(cx);
    assert!(redraws_over(cx, &renders, Duration::from_secs(1)) > 0);

    cx.simulate_window_visibility_change(harness.window(), WindowVisibility::Hidden);
    // The redraw scheduled before the window was hidden still lands, and draws it still.
    redraws_over(cx, &renders, Duration::from_secs(1));

    assert_eq!(redraws_over(cx, &renders, Duration::from_secs(1)), 0);
}

#[gpui::test]
fn a_spinner_resumes_once_its_window_is_shown_again(cx: &mut TestAppContext) {
    let (harness, renders) = spinner(cx);
    cx.update_window(harness.window(), |_, window, _| {
        spin_when_shown(window).detach();
    })
    .unwrap();
    cx.simulate_window_visibility_change(harness.window(), WindowVisibility::Hidden);
    redraws_over(cx, &renders, Duration::from_secs(1));

    cx.simulate_window_visibility_change(harness.window(), WindowVisibility::Visible);
    redraws_over(cx, &renders, Duration::from_millis(100));

    assert!(redraws_over(cx, &renders, Duration::from_secs(1)) > 0);
}

/// A spinning icon, and how many times it has rendered.
fn spinner(cx: &mut TestAppContext) -> (ComponentHarness, Rc<Cell<usize>>) {
    let renders = Rc::new(Cell::new(0));
    let counted = renders.clone();
    let harness = ComponentHarnessBuilder::default().build(cx, move |_, _| {
        counted.set(counted.get() + 1);
        Icon::new(IconName::LoaderCircle, black())
            .spin("spinner")
            .into_any_element()
    });
    (harness, renders)
}

/// How many times the spinner renders while `duration` passes in small steps.
fn redraws_over(cx: &mut TestAppContext, renders: &Cell<usize>, duration: Duration) -> usize {
    let before = renders.get();
    let step = Duration::from_millis(10);
    for _ in 0..duration.as_millis() / step.as_millis() {
        cx.executor().advance_clock(step);
        cx.run_until_parked();
    }
    renders.get() - before
}
