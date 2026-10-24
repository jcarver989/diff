use clankerdiff_gpui_ui::{
    picker::fuzzy_filter,
    prelude::{PickerItem, SearchPicker, SearchPickerEvent, UiTheme},
    testing::ComponentHarnessBuilder,
};
use clankerdiff_theme::ReviewTheme;
use gpui::{AppContext, Entity, IntoElement, TestAppContext};
use std::{cell::RefCell, rc::Rc};

#[test]
fn fuzzy_filter_ranks_matches_and_keeps_order_for_a_blank_query() {
    let keys = ["readme", "src/main.rs", "src/lib.rs"];
    assert_eq!(fuzzy_filter("", keys), vec![0, 1, 2]);
    assert_eq!(fuzzy_filter("lib", keys), vec![2]);
    assert_eq!(fuzzy_filter("zzz", keys), Vec::<usize>::new());
}

#[gpui::test]
fn search_picker_skips_disabled_items_and_filters_as_the_user_types(cx: &mut TestAppContext) {
    cx.update(gpui_base::init);
    let events = Rc::new(RefCell::new(Vec::new()));
    let picker: Rc<RefCell<Option<Entity<SearchPicker>>>> = Rc::default();
    let recorded = events.clone();
    let slot = picker.clone();
    ComponentHarnessBuilder::default().run(
        cx,
        move |window, cx| {
            slot.borrow_mut()
                .get_or_insert_with(|| {
                    let picker = cx.new(|cx| {
                        let items = vec![
                            PickerItem::new("alpha", "Alpha"),
                            PickerItem::new("beta", "Beta").disabled(true),
                            PickerItem::new("gamma", "Gamma"),
                        ];
                        SearchPicker::new(
                            "picker",
                            items,
                            UiTheme::new(&ReviewTheme::default()),
                            cx,
                        )
                        .searchable("Search…", window, cx)
                    });
                    let recorded = recorded.clone();
                    cx.subscribe(&picker, move |_, event: &SearchPickerEvent, _| {
                        recorded.borrow_mut().push(event.clone());
                    })
                    .detach();
                    picker
                })
                .clone()
                .into_any_element()
        },
        |harness, cx| {
            harness.press_key(cx, "down");
            harness.press_key(cx, "enter");
            harness.simulate_input(cx, "alp");
            assert!(harness.bounds(cx, "picker-gamma").is_none());
            harness.press_key(cx, "enter");
            harness.press_key(cx, "escape");
            assert_eq!(
                *events.borrow(),
                vec![
                    SearchPickerEvent::Chose(2),
                    SearchPickerEvent::Chose(0),
                    SearchPickerEvent::Dismissed
                ]
            );
            Ok(())
        },
    );
}
