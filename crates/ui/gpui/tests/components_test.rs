use clankerdiff_gpui_ui::{
    prelude::{
        Badge, Button, IconButton, IconName, ListRow, Modal, NoticeTone, SectionHeader, Segment, SegmentedControl,
        UiTheme,
    },
    testing::ComponentHarnessBuilder,
    tokens::TREE_INDENT,
};
use clankerdiff_theme::ReviewTheme;
use gpui::{AnyElement, IntoElement, ParentElement, TestAppContext, div, prelude::*, px};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[gpui::test]
fn a_full_width_button_spans_its_container(cx: &mut TestAppContext) {
    ComponentHarnessBuilder::default().run(
        cx,
        |_, _| div().w(px(300.0)).child(Button::new("wide", "Wide", theme()).full_width()).into_any_element(),
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
                ListRow::new(id, theme())
                    .depth(depth)
                    .child(div().debug_selector(move || format!("{id}-content")).child(id))
            };
            div().w(px(240.0)).child(row("parent", 0)).child(row("child", 2)).into_any_element()
        },
        |harness, cx| {
            let parent = harness.bounds(cx, "parent").ok_or("missing parent")?;
            let child = harness.bounds(cx, "child").ok_or("missing child")?;
            let parent_content = harness.bounds(cx, "parent-content").ok_or("missing parent content")?;
            let child_content = harness.bounds(cx, "child-content").ok_or("missing child content")?;
            assert_eq!(child.size.width, parent.size.width);
            assert_eq!(child_content.left() - parent_content.left(), px(2.0 * TREE_INDENT));
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
            let backdrop = harness.bounds(cx, "settings-backdrop").ok_or("missing backdrop")?;
            assert!(panel.bottom() < backdrop.center().y, "the panel sits near the top");
            harness.click(cx, "settings")?;
            assert_eq!(dismissals.get(), 0);
            harness.click(cx, "settings-backdrop")?;
            assert_eq!(dismissals.get(), 1);
            Ok(())
        },
    );
}

#[gpui::test]
fn a_badge_is_a_row_tall_pill_beside_its_neighbours(cx: &mut TestAppContext) {
    ComponentHarnessBuilder::default().run(
        cx,
        |_, _| {
            div()
                .flex()
                .child(Badge::new("left", "1h 42m", theme()).icon(IconName::Timer).tone(NoticeTone::Warning))
                .child(Badge::new("right", "Suspended", theme()))
                .into_any_element()
        },
        |harness, cx| {
            let left = harness.bounds(cx, "left").ok_or("missing left badge")?;
            let right = harness.bounds(cx, "right").ok_or("missing right badge")?;
            assert_eq!(left.size.height, harness.rem_size(cx) * 1.25);
            assert!(left.right() <= right.left(), "badges do not overlap");
            Ok(())
        },
    );
}

#[gpui::test]
fn clicking_a_section_header_toggles_it(cx: &mut TestAppContext) {
    let toggles = Rc::new(Cell::new(0));
    let (recorded, unused) = (toggles.clone(), Rc::new(Cell::new(0)));
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| section("cloud", &recorded, &unused, false),
        |harness, cx| {
            harness.click(cx, "cloud")?;
            harness.click(cx, "cloud-toggle")?;
            assert_eq!(toggles.get(), 2);
            Ok(())
        },
    );
}

#[gpui::test]
fn a_section_headers_actions_run_without_toggling_it(cx: &mut TestAppContext) {
    let (toggles, actions) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (toggle_count, action_count) = (toggles.clone(), actions.clone());
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| section("cloud", &toggle_count, &action_count, false),
        |harness, cx| {
            harness.hover(cx, "cloud")?;
            harness.click(cx, "cloud-add")?;
            assert_eq!((toggles.get(), actions.get()), (0, 1));
            Ok(())
        },
    );
}

#[gpui::test]
fn a_section_header_can_keep_its_actions_visible_without_hover(cx: &mut TestAppContext) {
    let (toggles, actions) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (toggle_count, action_count) = (toggles.clone(), actions.clone());
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| section("cloud", &toggle_count, &action_count, true),
        |harness, cx| {
            let badge = harness.bounds(cx, "cloud-badge").ok_or("missing badge")?;
            let action = harness.bounds(cx, "cloud-add").ok_or("missing action")?;
            assert!(badge.right() <= action.left(), "persistent actions reserve space beside the badge");
            harness.click(cx, "cloud-add")?;
            assert_eq!((toggles.get(), actions.get()), (0, 1));
            harness.hover(cx, "cloud")?;
            assert_eq!(harness.bounds(cx, "cloud-badge"), Some(badge));
            assert_eq!(harness.bounds(cx, "cloud-add"), Some(action));
            harness.click(cx, "cloud-add")?;
            assert_eq!((toggles.get(), actions.get()), (0, 2));
            Ok(())
        },
    );
}

#[gpui::test]
fn a_segmented_control_reports_enabled_choices_only(cx: &mut TestAppContext) {
    let chosen = Rc::new(RefCell::new(Vec::new()));
    let recorded = chosen.clone();
    ComponentHarnessBuilder::default().run(
        cx,
        move |_, _| {
            SegmentedControl::new("scope", theme())
                .segments([("left", false, false), ("middle", true, false), ("right", false, true)].map(
                    |(id, selected, disabled)| {
                        let chosen = recorded.clone();
                        Segment::new(id, id)
                            .selected(selected)
                            .disabled(disabled)
                            .on_click(move |_, _, _| chosen.borrow_mut().push(id))
                    },
                ))
                .into_any_element()
        },
        |harness, cx| {
            harness.click(cx, "left")?;
            harness.click(cx, "right")?;
            assert_eq!(*chosen.borrow(), vec!["left"]);
            Ok(())
        },
    );
}

fn section(id: &'static str, toggles: &Rc<Cell<usize>>, actions: &Rc<Cell<usize>>, always_visible: bool) -> AnyElement {
    let (toggles, actions) = (toggles.clone(), actions.clone());
    div()
        .w(px(240.0))
        .child(
            SectionHeader::new(id, "Cloud", true, theme())
                .actions_always_visible(always_visible)
                .icon(IconName::Cloud, theme().colors.info)
                .detail("3f2a")
                .badge(Badge::new(format!("{id}-badge"), "1h 42m", theme()))
                .on_toggle(move |_, _, _| toggles.set(toggles.get() + 1))
                .action(
                    IconButton::new(format!("{id}-add"), IconName::Plus, "New tab", theme())
                        .on_click(move |_, _, _| actions.set(actions.get() + 1)),
                ),
        )
        .into_any_element()
}

fn theme() -> UiTheme {
    UiTheme::new(&ReviewTheme::default())
}
