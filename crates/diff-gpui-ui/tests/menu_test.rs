use clankerdiff_gpui_ui::{
    prelude::{Button, MenuItem, UiTheme, with_menu},
    testing::ComponentHarnessBuilder,
};
use clankerdiff_theme::ReviewTheme;
use gpui::{IntoElement, MouseButton, TestAppContext};
use std::{cell::Cell, rc::Rc};

#[gpui::test]
fn clicking_the_trigger_opens_the_menu(cx: &mut TestAppContext) {
    cx.update(gpui_base::init);
    ComponentHarnessBuilder::default().run(
        cx,
        |_, _| menu(Rc::default()),
        |harness, cx| {
            assert!(harness.bounds(cx, "actions-menu").is_none());
            harness.click(cx, "actions")?;
            assert!(harness.bounds(cx, "actions-menu").is_some());
            Ok(())
        },
    );
}

#[gpui::test]
fn choosing_an_entry_closes_the_menu_and_runs_its_action(cx: &mut TestAppContext) {
    cx.update(gpui_base::init);
    let chosen = Rc::new(Cell::new(0));
    let recorded = chosen.clone();
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| menu(recorded.clone()),
        |harness, cx| {
            harness.click(cx, "actions")?;
            harness.click(cx, "rename")?;
            assert_eq!(chosen.get(), 1);
            assert!(harness.bounds(cx, "actions-menu").is_none());
            Ok(())
        },
    );
}

fn menu(chosen: Rc<Cell<usize>>) -> gpui::AnyElement {
    let theme = UiTheme::new(&ReviewTheme::default());
    let items = vec![MenuItem::new("rename", "Rename", move |_, _| {
        chosen.set(chosen.get() + 1);
    })];
    with_menu(
        "actions",
        Button::new("actions", "Actions", theme),
        MouseButton::Left,
        items,
        &theme,
    )
    .into_any_element()
}
