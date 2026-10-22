use clankerdiff_gpui_ui::{
    prelude::{Button, ListRow, UiTheme},
    testing::ComponentHarnessBuilder,
};
use clankerdiff_theme::ReviewTheme;
use gpui::{IntoElement, ParentElement, TestAppContext, div, prelude::*, px};
use std::{cell::Cell, rc::Rc};

#[gpui::test]
fn a_full_width_button_spans_its_container(cx: &mut TestAppContext) {
    ComponentHarnessBuilder::default().run(
        cx,
        |_, _| {
            div()
                .w(px(300.0))
                .child(Button::new("wide", "Wide", theme()).full_width())
                .into_any_element()
        },
        |harness, cx| {
            let button = harness.bounds(cx, "wide").ok_or("missing button")?;
            assert_eq!(button.size.width, px(300.0));
            Ok(())
        },
    );
}

#[gpui::test]
fn clicking_a_list_row_runs_its_handler(cx: &mut TestAppContext) {
    let clicks = Rc::new(Cell::new(0));
    let recorded = clicks.clone();
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| {
            let clicks = recorded.clone();
            ListRow::new("row", theme())
                .on_click(move |_, _, _| clicks.set(clicks.get() + 1))
                .child("Row")
                .into_any_element()
        },
        |harness, cx| {
            harness.click(cx, "row")?;
            assert_eq!(clicks.get(), 1);
            Ok(())
        },
    );
}

fn theme() -> UiTheme {
    UiTheme::new(&ReviewTheme::default())
}
