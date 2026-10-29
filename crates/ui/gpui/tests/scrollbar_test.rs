use clankerdiff_gpui_ui::{
    prelude::{ListScrollbar, UiTheme},
    testing::ComponentHarnessBuilder,
};
use clankerdiff_ui_core::ReviewTheme;
use gpui::{AnyElement, ListAlignment, ListState, TestAppContext, div, list, point, prelude::*, px};

#[gpui::test]
fn a_list_that_fits_its_viewport_has_no_scrollbar(cx: &mut TestAppContext) {
    let rows = ListState::new(3, ListAlignment::Top, px(0.0));
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| layout(std::slice::from_ref(&rows)),
        |harness, cx| {
            assert!(harness.bounds(cx, "scrollbar-0").is_none());
            Ok(())
        },
    );
}

#[gpui::test]
fn a_long_list_shows_a_scrollbar_at_its_top(cx: &mut TestAppContext) {
    let rows = ListState::new(100, ListAlignment::Top, px(0.0)).measure_all();
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| layout(std::slice::from_ref(&rows)),
        |harness, cx| {
            let track = harness.bounds(cx, "scrollbar-0").ok_or("missing scrollbar")?;
            let thumb = harness.bounds(cx, "scrollbar-0-thumb").ok_or("missing thumb")?;
            assert_eq!(track.size.height, px(600.0));
            assert_eq!(thumb.top(), track.top());
            Ok(())
        },
    );
}

#[gpui::test]
fn dragging_the_thumb_to_the_bottom_scrolls_to_the_end(cx: &mut TestAppContext) {
    let rows = ListState::new(100, ListAlignment::Top, px(0.0)).measure_all();
    let rendered = rows.clone();
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| layout(std::slice::from_ref(&rendered)),
        |harness, cx| {
            let track = harness.bounds(cx, "scrollbar-0").ok_or("missing scrollbar")?;
            harness.drag(cx, "scrollbar-0-thumb", point(track.center().x, track.bottom()))?;
            assert_eq!(offset(&rows), rows.max_offset_for_scrollbar().y);
            assert!(!rows.is_scrollbar_dragging(), "releasing the thumb ends the drag");
            let thumb = harness.bounds(cx, "scrollbar-0-thumb").ok_or("missing thumb")?;
            assert_eq!(thumb.bottom(), track.bottom());
            Ok(())
        },
    );
}

#[gpui::test]
fn clicking_the_track_jumps_there(cx: &mut TestAppContext) {
    let rows = ListState::new(100, ListAlignment::Top, px(0.0)).measure_all();
    let rendered = rows.clone();
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| layout(std::slice::from_ref(&rendered)),
        |harness, cx| {
            harness.click(cx, "scrollbar-0")?;
            let fraction = offset(&rows) / rows.max_offset_for_scrollbar().y;
            assert!((fraction - 0.5).abs() < 0.01, "expected halfway, got {fraction}");
            Ok(())
        },
    );
}

#[gpui::test]
fn dragging_one_scrollbar_leaves_another_list_alone(cx: &mut TestAppContext) {
    let first = ListState::new(100, ListAlignment::Top, px(0.0)).measure_all();
    let second = ListState::new(100, ListAlignment::Top, px(0.0)).measure_all();
    let rendered = [first.clone(), second.clone()];
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| layout(&rendered),
        |harness, cx| {
            let track = harness.bounds(cx, "scrollbar-1").ok_or("missing scrollbar")?;
            harness.drag(cx, "scrollbar-1-thumb", point(track.center().x, track.bottom()))?;
            assert_eq!(offset(&first), px(0.0));
            assert_eq!(offset(&second), second.max_offset_for_scrollbar().y);
            Ok(())
        },
    );
}

fn layout(lists: &[ListState]) -> AnyElement {
    let theme = UiTheme::new(&ReviewTheme::default());
    div()
        .size_full()
        .flex()
        .children(lists.iter().enumerate().map(|(index, rows)| {
            div()
                .flex_1()
                .h_full()
                .flex()
                .child(list(rows.clone(), |_, _, _| div().h(px(40.0)).into_any_element()).flex_1().h_full())
                .child(ListScrollbar::new(
                    gpui::ElementId::Name(format!("scrollbar-{index}").into()),
                    rows.clone(),
                    theme,
                ))
        }))
        .into_any_element()
}

fn offset(rows: &ListState) -> gpui::Pixels {
    -rows.scroll_px_offset_for_scrollbar().y
}
