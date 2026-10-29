use clankerdiff_gpui_ui::{
    prelude::{IconName, TreeRow, UiTheme},
    testing::ComponentHarnessBuilder,
    tokens::ROW_HEIGHT,
};
use clankerdiff_ui_core::ReviewTheme;
use gpui::{IntoElement, ParentElement, TestAppContext, div, prelude::*, px};
use std::{cell::Cell, rc::Rc};

#[gpui::test]
fn rows_are_one_row_tall_and_scale_with_the_interface_size(cx: &mut TestAppContext) {
    for interface_size in [16.0, 20.0] {
        ComponentHarnessBuilder { interface_size, ..ComponentHarnessBuilder::default() }.run(
            cx,
            |_, _| tree([row("src", 0).expanded(true), row("lib.rs", 1)]),
            |harness, cx| {
                let row = harness.bounds(cx, "src").ok_or("missing row")?;
                assert_eq!(row.size.height, px(ROW_HEIGHT / 16.0 * interface_size));
                Ok(())
            },
        );
    }
}

#[gpui::test]
fn clicking_a_row_runs_its_handler(cx: &mut TestAppContext) {
    let clicks = Rc::new(Cell::new(0));
    let recorded = clicks.clone();
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| {
            let clicks = recorded.clone();
            tree([row("src", 0)
                .expanded(false)
                .icon(IconName::Folder, None)
                .on_click(move |_, _, _| clicks.set(clicks.get() + 1))])
        },
        |harness, cx| {
            harness.click(cx, "src")?;
            harness.click(cx, "src")?;
            assert_eq!(clicks.get(), 2);
            Ok(())
        },
    );
}

fn tree<const N: usize>(rows: [TreeRow; N]) -> gpui::AnyElement {
    div().w(px(240.0)).children(rows).into_any_element()
}

fn row(label: &'static str, depth: usize) -> TreeRow {
    TreeRow::new(label, label, UiTheme::new(&ReviewTheme::default())).depth(depth)
}
