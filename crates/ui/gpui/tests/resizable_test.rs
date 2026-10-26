use clankerdiff_gpui_ui::{
    prelude::{PanelSide, ResizablePanel, UiTheme},
    testing::ComponentHarnessBuilder,
    tokens::rem,
};
use clankerdiff_theme::ReviewTheme;
use gpui::{IntoElement, ParentElement, Rems, TestAppContext, div, point, prelude::*, px, rems};
use std::{cell::Cell, rc::Rc};

#[gpui::test]
fn a_panel_is_as_wide_as_its_owner_says(cx: &mut TestAppContext) {
    for interface_size in [16.0, 20.0] {
        ComponentHarnessBuilder { interface_size, ..ComponentHarnessBuilder::default() }.run(
            cx,
            |_, _| layout(panel(rems(15.0), PanelSide::Left)),
            |harness, cx| {
                let bounds = harness.bounds(cx, "panel").ok_or("missing panel")?;
                assert_eq!(bounds.size.width, px(15.0 * interface_size));
                Ok(())
            },
        );
    }
}

#[gpui::test]
fn dragging_the_handle_reports_the_new_width_in_rems(cx: &mut TestAppContext) {
    let width = Rc::new(Cell::new(rems(15.0)));
    let rendered = width.clone();
    ComponentHarnessBuilder { interface_size: 20.0, ..ComponentHarnessBuilder::default() }.run(
        cx,
        move |_, _| {
            let owner = rendered.clone();
            layout(panel(rendered.get(), PanelSide::Left).on_resize(move |width, _, _| owner.set(width)))
        },
        |harness, cx| {
            let handle = harness.bounds(cx, "panel-resize-handle").ok_or("missing handle")?;
            harness.drag(cx, "panel-resize-handle", point(px(400.0), handle.center().y))?;
            assert_rems(width.get(), rems(20.0));
            let panel = harness.bounds(cx, "panel").ok_or("missing panel")?;
            assert_eq!(panel.size.width, px(400.0));
            Ok(())
        },
    );
}

#[gpui::test]
fn a_right_panel_grows_as_its_left_edge_is_dragged_left(cx: &mut TestAppContext) {
    let width = Rc::new(Cell::new(rem(200.0)));
    let rendered = width.clone();
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| {
            let owner = rendered.clone();
            div()
                .size_full()
                .flex()
                .child(div().flex_1())
                .child(panel(rendered.get(), PanelSide::Right).on_resize(move |width, _, _| owner.set(width)))
                .into_any_element()
        },
        |harness, cx| {
            let handle = harness.bounds(cx, "panel-resize-handle").ok_or("missing handle")?;
            harness.drag(cx, "panel-resize-handle", point(px(500.0), handle.center().y))?;
            assert_rems(width.get(), rem(300.0));
            Ok(())
        },
    );
}

#[gpui::test]
fn resizing_is_clamped_to_the_panel_bounds(cx: &mut TestAppContext) {
    let width = Rc::new(Cell::new(rem(200.0)));
    let rendered = width.clone();
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| {
            let owner = rendered.clone();
            layout(
                panel(rendered.get(), PanelSide::Left)
                    .min_width(rem(160.0))
                    .max_width(rem(320.0))
                    .on_resize(move |width, _, _| owner.set(width)),
            )
        },
        |harness, cx| {
            let y = harness.bounds(cx, "panel-resize-handle").ok_or("missing handle")?.center().y;
            harness.drag(cx, "panel-resize-handle", point(px(700.0), y))?;
            assert_rems(width.get(), rem(320.0));
            harness.drag(cx, "panel-resize-handle", point(px(20.0), y))?;
            assert_rems(width.get(), rem(160.0));
            Ok(())
        },
    );
}

#[gpui::test]
fn double_clicking_the_handle_resets_the_panel(cx: &mut TestAppContext) {
    let resets = Rc::new(Cell::new(0));
    let recorded = resets.clone();
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| {
            let resets = recorded.clone();
            layout(panel(rem(200.0), PanelSide::Left).on_reset(move |_, _| resets.set(resets.get() + 1)))
        },
        |harness, cx| {
            harness.click(cx, "panel-resize-handle")?;
            assert_eq!(resets.get(), 0);
            harness.double_click(cx, "panel-resize-handle")?;
            assert_eq!(resets.get(), 1);
            Ok(())
        },
    );
}

fn layout(panel: ResizablePanel) -> gpui::AnyElement {
    div().size_full().flex().child(panel).child(div().flex_1()).into_any_element()
}

fn panel(width: Rems, side: PanelSide) -> ResizablePanel {
    ResizablePanel::new("panel", side, width, UiTheme::new(&ReviewTheme::default())).child(div().size_full())
}

fn assert_rems(actual: Rems, expected: Rems) {
    assert!((actual.0 - expected.0).abs() < 0.01, "expected {expected:?}, got {actual:?}");
}
