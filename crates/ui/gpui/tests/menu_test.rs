use clankerdiff_gpui_ui::{
    prelude::{Button, IconName, MenuItem, UiTheme, with_menu},
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

#[gpui::test]
fn a_closed_menu_builds_no_entries(cx: &mut TestAppContext) {
    cx.update(gpui_base::init);
    let builds = Rc::new(Cell::new(0));
    let counted = builds.clone();
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| {
            let counted = counted.clone();
            menu_with(move || {
                counted.set(counted.get() + 1);
                Vec::new()
            })
        },
        |harness, cx| {
            assert_eq!(builds.get(), 0, "a closed menu builds no entries");
            harness.click(cx, "actions")?;
            assert!(builds.get() > 0, "an open menu builds its entries");
            Ok(())
        },
    );
}

#[gpui::test]
fn a_destructive_entry_past_a_separator_runs_its_action(cx: &mut TestAppContext) {
    cx.update(gpui_base::init);
    let chosen = Rc::new(Cell::new(0));
    let recorded = chosen.clone();
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| {
            let recorded = recorded.clone();
            menu_with(move || {
                vec![
                    MenuItem::new("rename", "Rename", |_, _| {}),
                    MenuItem::separator(),
                    MenuItem::new("delete", "Delete", move |_, _| {
                        recorded.set(recorded.get() + 1);
                    })
                    .icon(IconName::X)
                    .destructive(),
                ]
            })
        },
        |harness, cx| {
            harness.click(cx, "actions")?;
            let rename = harness.bounds(cx, "rename").ok_or("missing rename")?;
            let delete = harness.bounds(cx, "delete").ok_or("missing delete")?;
            assert!(
                delete.top() > rename.bottom(),
                "the separator sits between them"
            );
            harness.click(cx, "delete")?;
            assert_eq!(chosen.get(), 1);
            Ok(())
        },
    );
}

fn menu(chosen: Rc<Cell<usize>>) -> gpui::AnyElement {
    menu_with(move || {
        vec![MenuItem::new("rename", "Rename", move |_, _| {
            chosen.set(chosen.get() + 1);
        })]
    })
}

fn menu_with(items: impl FnOnce() -> Vec<MenuItem> + 'static) -> gpui::AnyElement {
    let theme = UiTheme::new(&ReviewTheme::default());
    with_menu(
        "actions",
        Button::new("actions", "Actions", theme),
        MouseButton::Left,
        move |_, _| items(),
        &theme,
    )
    .into_any_element()
}
