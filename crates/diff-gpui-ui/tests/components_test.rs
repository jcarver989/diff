use clankerdiff_gpui_ui::{
    prelude::{Button, ListRow, Modal, UiTheme},
    testing::ComponentHarnessBuilder,
    tokens::TREE_INDENT,
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

#[gpui::test]
fn a_nested_row_indents_its_content_but_keeps_its_full_width(cx: &mut TestAppContext) {
    ComponentHarnessBuilder::default().run(
        cx,
        |_, _| {
            let row = |id: &'static str, depth| {
                ListRow::new(id, theme()).depth(depth).child(
                    div()
                        .debug_selector(move || format!("{id}-content"))
                        .child(id),
                )
            };
            div()
                .w(px(240.0))
                .child(row("parent", 0))
                .child(row("child", 2))
                .into_any_element()
        },
        |harness, cx| {
            let parent = harness.bounds(cx, "parent").ok_or("missing parent")?;
            let child = harness.bounds(cx, "child").ok_or("missing child")?;
            let parent_content = harness
                .bounds(cx, "parent-content")
                .ok_or("missing parent content")?;
            let child_content = harness
                .bounds(cx, "child-content")
                .ok_or("missing child content")?;
            assert_eq!(child.size.width, parent.size.width);
            assert_eq!(
                child_content.left() - parent_content.left(),
                px(2.0 * TREE_INDENT)
            );
            Ok(())
        },
    );
}

#[gpui::test]
fn a_top_aligned_modal_is_dismissed_by_its_scrim_but_not_by_its_panel(cx: &mut TestAppContext) {
    let dismissals = Rc::new(Cell::new(0));
    let counted = dismissals.clone();
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| {
            let counted = counted.clone();
            Modal::new("settings", "Settings", theme())
                .align_top()
                .on_dismiss(move |_, _| counted.set(counted.get() + 1))
                .into_any_element()
        },
        |harness, cx| {
            let panel = harness.bounds(cx, "settings").ok_or("missing panel")?;
            let backdrop = harness
                .bounds(cx, "settings-backdrop")
                .ok_or("missing backdrop")?;
            assert!(
                panel.bottom() < backdrop.center().y,
                "the panel sits near the top"
            );
            harness.click(cx, "settings")?;
            assert_eq!(dismissals.get(), 0);
            harness.click(cx, "settings-backdrop")?;
            assert_eq!(dismissals.get(), 1);
            Ok(())
        },
    );
}

fn theme() -> UiTheme {
    UiTheme::new(&ReviewTheme::default())
}
