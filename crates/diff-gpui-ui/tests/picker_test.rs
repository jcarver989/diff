use clankerdiff_gpui_ui::{
    picker::fuzzy_filter,
    prelude::{
        IconName, PickerItem, SearchPicker, SearchPickerEvent, ThemePicker, ThemePickerEvent,
        UiTheme,
    },
    testing::{ComponentHarness, ComponentHarnessBuilder},
};
use clankerdiff_theme::{Appearance, ReviewTheme, ThemeChoice, ThemeId};
use gpui::{AppContext, Entity, IntoElement, TestAppContext};
use std::{cell::RefCell, rc::Rc, sync::Arc};

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
                    SearchPickerEvent::Highlighted(2),
                    SearchPickerEvent::Chose(2),
                    SearchPickerEvent::Highlighted(0),
                    SearchPickerEvent::Chose(0),
                    SearchPickerEvent::Dismissed
                ]
            );
            Ok(())
        },
    );
}

#[gpui::test]
fn grouped_items_stay_together_under_their_headings_however_a_search_ranks_them(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_base::init);
    let picker: Rc<RefCell<Option<Entity<SearchPicker>>>> = Rc::default();
    ComponentHarnessBuilder::default().run(
        cx,
        move |window, cx| {
            picker
                .borrow_mut()
                .get_or_insert_with(|| {
                    cx.new(|cx| {
                        let items = vec![
                            PickerItem::new("one", "cloud one").group("Running"),
                            PickerItem::new("new", "cloud")
                                .group("Launch")
                                .icon(IconName::Plus),
                            PickerItem::new("two", "cloud two").group("Running"),
                        ];
                        SearchPicker::new(
                            "picker",
                            items,
                            UiTheme::new(&ReviewTheme::default()),
                            cx,
                        )
                        .searchable("Search…", window, cx)
                    })
                })
                .clone()
                .into_any_element()
        },
        |harness, cx| {
            harness.simulate_input(cx, "cloud");
            let running = harness
                .bounds(cx, "picker-group-Running")
                .ok_or("missing Running heading")?;
            let launch = harness
                .bounds(cx, "picker-group-Launch")
                .ok_or("missing Launch heading")?;
            let one = harness.bounds(cx, "picker-one").ok_or("missing one")?;
            let two = harness.bounds(cx, "picker-two").ok_or("missing two")?;
            let new = harness.bounds(cx, "picker-new").ok_or("missing new")?;
            assert!(running.bottom() <= one.top());
            assert!(one.bottom() <= two.top());
            assert!(two.bottom() <= launch.top());
            assert!(launch.bottom() <= new.top());
            Ok(())
        },
    );
}

#[gpui::test]
fn the_theme_picker_previews_the_highlighted_theme_and_commits_it(cx: &mut TestAppContext) {
    let themes = ThemeChoice::catalog();
    let (harness, events) = theme_picker(cx, Arc::clone(&themes));
    harness.press_key(cx, "down");
    harness.press_key(cx, "enter");
    let next = themes[position(&themes, &ReviewTheme::default()) + 1]
        .theme
        .id()
        .clone();
    assert_eq!(
        *events.borrow(),
        vec![Event::Preview(next.clone()), Event::Commit(next)]
    );
}

#[gpui::test]
fn escape_cancels_the_theme_picker_back_to_the_original_theme(cx: &mut TestAppContext) {
    let (harness, events) = theme_picker(cx, ThemeChoice::catalog());
    harness.press_key(cx, "down");
    harness.press_key(cx, "escape");
    harness.press_key(cx, "escape");
    let events = events.borrow();
    assert_eq!(events.len(), 2, "{events:?}");
    assert_eq!(
        events[1],
        Event::Cancel(ReviewTheme::default().id().clone())
    );
}

#[gpui::test]
fn the_theme_picker_lists_only_the_themes_it_is_given_and_filters_them(cx: &mut TestAppContext) {
    let (harness, events) = theme_picker(cx, ThemeChoice::catalog_for(Appearance::Light));
    assert!(harness.bounds(cx, "themes-github-light").is_some());
    assert!(harness.bounds(cx, "themes-github-dark").is_none());
    harness.simulate_input(cx, "solarized");
    assert!(harness.bounds(cx, "themes-github-light").is_none());
    harness.press_key(cx, "enter");
    assert_eq!(
        events.borrow().last(),
        Some(&Event::Commit(ThemeId::Builtin("solarized-light".into())))
    );
}

#[derive(Debug, PartialEq)]
enum Event {
    Preview(ThemeId),
    Commit(ThemeId),
    Cancel(ThemeId),
}

fn theme_picker(
    cx: &mut TestAppContext,
    themes: Arc<[ThemeChoice]>,
) -> (ComponentHarness, Rc<RefCell<Vec<Event>>>) {
    cx.update(gpui_base::init);
    let events = Rc::new(RefCell::new(Vec::new()));
    let picker: Rc<RefCell<Option<Entity<ThemePicker>>>> = Rc::default();
    let recorded = events.clone();
    let harness = ComponentHarnessBuilder::default().build(cx, move |window, cx| {
        picker
            .borrow_mut()
            .get_or_insert_with(|| {
                let current = ReviewTheme::default();
                let ui = UiTheme::new(&current);
                let themes = themes.clone();
                let picker =
                    cx.new(|cx| ThemePicker::new("themes", &current, themes, ui, window, cx));
                let recorded = recorded.clone();
                cx.subscribe(&picker, move |_, event: &ThemePickerEvent, _| {
                    recorded.borrow_mut().push(match event {
                        ThemePickerEvent::Preview(theme) => Event::Preview(theme.id().clone()),
                        ThemePickerEvent::Commit(theme) => Event::Commit(theme.id().clone()),
                        ThemePickerEvent::Cancel(theme) => Event::Cancel(theme.id().clone()),
                    });
                })
                .detach();
                picker
            })
            .clone()
            .into_any_element()
    });
    (harness, events)
}

fn position(themes: &[ThemeChoice], theme: &ReviewTheme) -> usize {
    themes
        .iter()
        .position(|choice| choice.theme.id() == theme.id())
        .expect("the theme is listed")
}
