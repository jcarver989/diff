use clankerdiff_gpui_ui::{
    InterfaceSize,
    picker::fuzzy_filter,
    prelude::{
        IconName, PickerDetail, PickerItem, PickerMarker, SearchPicker, SearchPickerEvent, ThemePicker,
        ThemePickerEvent, Typography, UiTheme,
    },
    testing::ViewHarness,
    tokens::{
        BASE_SIZE, FLOATING_INSET, PICKER_LIST_HEIGHT, PICKER_PREVIEW_HEIGHT, PICKER_PREVIEW_WIDTH, POPOVER_WIDTH,
        ROW_HEIGHT, rem,
    },
};
use clankerdiff_theme::{Appearance, ReviewTheme, ThemeChoice, ThemeId};
use gpui::{
    AppContext, Context, InteractiveElement, IntoElement, ParentElement, TestAppContext, Window, WindowOptions, div, px,
};
use std::sync::Arc;

#[test]
fn fuzzy_filter_ranks_matches_and_keeps_order_for_a_blank_query() {
    let keys = ["readme", "src/main.rs", "src/lib.rs"];
    assert_eq!(fuzzy_filter("", keys), vec![0, 1, 2]);
    assert_eq!(fuzzy_filter("lib", keys), vec![2]);
    assert_eq!(fuzzy_filter("zzz", keys), Vec::<usize>::new());
}

#[gpui::test]
fn search_picker_skips_disabled_items_and_filters_as_the_user_types(cx: &mut TestAppContext) {
    let harness = search_picker(cx, |window, cx| {
        let items = vec![
            PickerItem::new("alpha", "Alpha"),
            PickerItem::new("beta", "Beta").disabled(true),
            PickerItem::new("gamma", "Gamma"),
        ];
        SearchPicker::new("picker", items, theme(), cx).searchable("Search…", window, cx)
    });
    harness.press_key(cx, "down");
    harness.press_key(cx, "enter");
    harness.simulate_input(cx, "alp");
    assert!(harness.bounds(cx, "picker-gamma").is_none());
    harness.press_key(cx, "enter");
    harness.press_key(cx, "escape");
    assert_eq!(
        harness.events(cx),
        vec![
            SearchPickerEvent::Highlighted(2),
            SearchPickerEvent::Chose(2),
            SearchPickerEvent::Highlighted(0),
            SearchPickerEvent::Chose(0),
            SearchPickerEvent::Dismissed
        ]
    );
}

#[gpui::test]
fn a_subtitle_shows_under_the_label_and_can_be_searched(cx: &mut TestAppContext) {
    let harness = search_picker(cx, |window, cx| {
        let items = vec![
            PickerItem::new("parser", "Add parser").subtitle("a1f3c09 · 2h ago"),
            PickerItem::new("session", "Wire session").subtitle("7be20d4 · 3d ago"),
        ];
        SearchPicker::new("picker", items, theme(), cx).searchable("Search…", window, cx)
    });
    assert!(harness.bounds(cx, "picker-parser").is_some());
    harness.simulate_input(cx, "7be2");
    assert!(harness.bounds(cx, "picker-parser").is_none());
    assert!(harness.bounds(cx, "picker-session").is_some());
}

#[gpui::test]
fn prefixes_form_a_column_and_details_can_be_searched(cx: &mut TestAppContext) {
    search_picker(cx, |window, cx| {
        let theme = theme();
        let items = vec![
            PickerItem::new("parser", "Add parser")
                .prefix("a1f3c09")
                .detail("2h")
                .detail(PickerDetail::new("+12").code().color(theme.colors.positive))
                .checked(true),
            PickerItem::new("draft", "Draft").detail("now"),
        ];
        SearchPicker::new("picker", items, theme, cx).searchable("Search…", window, cx)
    })
    .run(cx, |harness, cx| {
        let row = harness.bounds(cx, "picker-parser").ok_or("missing row")?;
        let parser = harness.bounds(cx, "picker-parser-label").ok_or("missing label")?;
        let draft = harness.bounds(cx, "picker-draft-label").ok_or("missing label")?;
        assert_eq!(parser.left(), draft.left());
        assert_eq!(row.size.height, px(ROW_HEIGHT));
        harness.simulate_input(cx, "+12");
        assert!(harness.bounds(cx, "picker-draft").is_none());
        assert!(harness.bounds(cx, "picker-parser").is_some());
        Ok(())
    });
}

#[gpui::test]
fn markers_lead_the_label_and_checks_trail_it_in_shared_columns(cx: &mut TestAppContext) {
    search_picker(cx, |_, cx| {
        let theme = theme();
        let items = vec![
            PickerItem::new("done", "Done").marker(PickerMarker::Dot(theme.colors.accent)).checked(true),
            PickerItem::new("draft", "Draft").marker(PickerMarker::Pending),
        ];
        SearchPicker::new("picker", items, theme, cx)
    })
    .run(cx, |harness, cx| {
        let marker = harness.bounds(cx, "picker-done-marker").ok_or("missing marker")?;
        let label = harness.bounds(cx, "picker-done-label").ok_or("missing label")?;
        let check = harness.bounds(cx, "picker-done-check").ok_or("missing check")?;
        let draft_label = harness.bounds(cx, "picker-draft-label").ok_or("missing draft label")?;
        let draft_check = harness.bounds(cx, "picker-draft-check").ok_or("missing draft check slot")?;
        assert!(marker.right() <= label.left());
        assert!(label.right() <= check.left());
        assert_eq!(label.left(), draft_label.left());
        assert_eq!(check.left(), draft_check.left());
        Ok(())
    });
}

#[gpui::test]
fn a_preview_sits_beside_the_list_and_follows_the_highlight(cx: &mut TestAppContext) {
    search_picker(cx, |_, cx| {
        let items = vec![PickerItem::new("alpha", "Alpha"), PickerItem::new("beta", "Beta")];
        SearchPicker::new("picker", items, theme(), cx)
            .width(rem(PICKER_PREVIEW_WIDTH))
            .height(rem(PICKER_PREVIEW_HEIGHT))
            .preview(|index, _, _| {
                div()
                    .debug_selector(move || format!("preview-{index}"))
                    .child(format!("Item {index}"))
                    .into_any_element()
            })
    })
    .run(cx, |harness, cx| {
        let list = harness.bounds(cx, "picker-list").ok_or("missing list")?;
        let preview = harness.bounds(cx, "picker-preview").ok_or("missing preview")?;
        assert!(list.right() <= preview.left());
        assert!(harness.bounds(cx, "preview-0").is_some());
        harness.update(cx, |picker, cx| picker.move_highlight(1, cx));
        harness.draw(cx);
        assert!(harness.bounds(cx, "preview-0").is_none());
        assert!(harness.bounds(cx, "preview-1").is_some());
        assert_eq!(harness.events(cx), vec![SearchPickerEvent::Highlighted(1)]);
        Ok(())
    });
}

#[gpui::test]
fn hovering_an_item_highlights_it_and_updates_the_preview(cx: &mut TestAppContext) {
    search_picker(cx, |_, cx| {
        let items = vec![
            PickerItem::new("alpha", "Alpha"),
            PickerItem::new("beta", "Beta").disabled(true),
            PickerItem::new("gamma", "Gamma"),
        ];
        SearchPicker::new("picker", items, theme(), cx)
            .width(rem(PICKER_PREVIEW_WIDTH))
            .height(rem(PICKER_PREVIEW_HEIGHT))
            .preview(|index, _, _| {
                div()
                    .debug_selector(move || format!("preview-{index}"))
                    .child(format!("Item {index}"))
                    .into_any_element()
            })
    })
    .run(cx, |harness, cx| {
        harness.hover(cx, "picker-gamma")?;
        harness.draw(cx);
        assert!(harness.bounds(cx, "preview-0").is_none());
        assert!(harness.bounds(cx, "preview-2").is_some());
        harness.hover(cx, "picker-beta")?;
        harness.draw(cx);
        assert!(harness.bounds(cx, "preview-2").is_some());
        harness.hover(cx, "picker-alpha")?;
        assert_eq!(harness.events(cx), vec![SearchPickerEvent::Highlighted(2), SearchPickerEvent::Highlighted(0)]);
        Ok(())
    });
}

#[gpui::test]
fn subtitle_rows_keep_their_content_height_when_the_list_overflows(cx: &mut TestAppContext) {
    for interface_size in [BASE_SIZE, 20.0] {
        let mut short_row_height = None;
        for item_count in [3, 42] {
            search_picker_sized(cx, interface_size, move |_, cx| {
                let items = (0..item_count)
                    .map(|index| {
                        let item = PickerItem::new(
                            format!("item-{index}"),
                            "A long commit subject that must truncate instead of wrapping ".repeat(3),
                        );
                        if index % 3 == 2 {
                            item
                        } else {
                            item.subtitle("a1f3c09 · 2 days ago · +20998 −3846 ".repeat(3))
                        }
                    })
                    .collect();
                SearchPicker::new("picker", items, theme(), cx)
                    .width(rem(POPOVER_WIDTH))
                    .max_list_height(rem(PICKER_LIST_HEIGHT))
            })
            .run(cx, |harness, cx| {
                let row = harness.bounds(cx, "picker-item-0").ok_or("missing row")?;
                let label = harness.bounds(cx, "picker-item-0-label").ok_or("missing label")?;
                let subtitle = harness.bounds(cx, "picker-item-0-subtitle").ok_or("missing subtitle")?;
                let next = harness.bounds(cx, "picker-item-1").ok_or("missing next row")?;
                let single = harness.bounds(cx, "picker-item-2").ok_or("missing single-line row")?;
                let list = harness.bounds(cx, "picker-list").ok_or("missing list")?;
                let scale = interface_size / BASE_SIZE;
                assert!(label.size.height >= px(Typography::Body.line_height() * scale));
                assert!(subtitle.size.height >= px(Typography::Small.line_height() * scale));
                assert!(row.top() <= label.top());
                assert!(label.bottom() <= subtitle.top());
                assert!(subtitle.bottom() <= row.bottom(), "subtitle extends beyond its row: {row:?}");
                assert!(row.bottom() <= next.top());
                assert!(next.bottom() <= single.top());
                assert!(row.left() <= label.left() && label.right() <= row.right());
                assert!(row.left() <= subtitle.left() && subtitle.right() <= row.right());
                assert_eq!(single.size.height, px(ROW_HEIGHT * scale));
                assert!(list.size.height <= px(PICKER_LIST_HEIGHT * scale));
                if let Some(height) = short_row_height {
                    assert_eq!(row.size.height, height, "overflow must scroll, not shrink rows");
                } else {
                    short_row_height = Some(row.size.height);
                }
                Ok(())
            });
        }
    }
}

#[gpui::test]
fn rows_sit_one_floating_inset_inside_the_panel_so_their_corners_are_concentric(cx: &mut TestAppContext) {
    search_picker(cx, |_, cx| SearchPicker::new("picker", vec![PickerItem::new("alpha", "Alpha")], theme(), cx)).run(
        cx,
        |harness, cx| {
            let panel = harness.bounds(cx, "picker").ok_or("missing picker")?;
            let row = harness.bounds(cx, "picker-alpha").ok_or("missing row")?;
            let border = px(1.0);
            assert_eq!(row.left() - panel.left(), px(FLOATING_INSET) + border);
            assert_eq!(row.top() - panel.top(), px(FLOATING_INSET) + border);
            Ok(())
        },
    );
}

#[gpui::test]
fn grouped_items_stay_together_under_their_headings_however_a_search_ranks_them(cx: &mut TestAppContext) {
    search_picker(cx, |window, cx| {
        let items = vec![
            PickerItem::new("one", "cloud one").group("Running"),
            PickerItem::new("new", "cloud").group("Launch").icon(IconName::Plus),
            PickerItem::new("two", "cloud two").group("Running"),
        ];
        SearchPicker::new("picker", items, theme(), cx).searchable("Search…", window, cx)
    })
    .run(cx, |harness, cx| {
        harness.simulate_input(cx, "cloud");
        let running = harness.bounds(cx, "picker-group-Running").ok_or("missing Running heading")?;
        let launch = harness.bounds(cx, "picker-group-Launch").ok_or("missing Launch heading")?;
        let one = harness.bounds(cx, "picker-one").ok_or("missing one")?;
        let two = harness.bounds(cx, "picker-two").ok_or("missing two")?;
        let new = harness.bounds(cx, "picker-new").ok_or("missing new")?;
        assert!(running.bottom() <= one.top());
        assert!(one.bottom() <= two.top());
        assert!(two.bottom() <= launch.top());
        assert!(launch.bottom() <= new.top());
        Ok(())
    });
}

#[gpui::test]
fn the_theme_picker_previews_the_highlighted_theme_and_commits_it(cx: &mut TestAppContext) {
    let themes = ThemeChoice::catalog();
    let harness = theme_picker(cx, Arc::clone(&themes));
    harness.press_key(cx, "down");
    harness.press_key(cx, "enter");
    let next = themes[position(&themes, &ReviewTheme::default()) + 1].theme.id().clone();
    assert_eq!(theme_events(&harness, cx), vec![Event::Preview(next.clone()), Event::Commit(next)]);
}

#[gpui::test]
fn escape_cancels_the_theme_picker_back_to_the_original_theme(cx: &mut TestAppContext) {
    let harness = theme_picker(cx, ThemeChoice::catalog());
    harness.press_key(cx, "down");
    harness.press_key(cx, "escape");
    harness.press_key(cx, "escape");
    let events = theme_events(&harness, cx);
    assert_eq!(events.len(), 2, "{events:?}");
    assert_eq!(events[1], Event::Cancel(ReviewTheme::default().id().clone()));
}

#[gpui::test]
fn the_theme_picker_lists_only_the_themes_it_is_given_and_filters_them(cx: &mut TestAppContext) {
    let harness = theme_picker(cx, ThemeChoice::catalog_for(Appearance::Light));
    assert!(harness.bounds(cx, "themes-github-light").is_some());
    assert!(harness.bounds(cx, "themes-github-dark").is_none());
    harness.simulate_input(cx, "solarized");
    assert!(harness.bounds(cx, "themes-github-light").is_none());
    harness.press_key(cx, "enter");
    assert_eq!(theme_events(&harness, cx).last(), Some(&Event::Commit(ThemeId::Builtin("solarized-light".into()))));
}

#[derive(Debug, PartialEq)]
enum Event {
    Preview(ThemeId),
    Commit(ThemeId),
    Cancel(ThemeId),
}

fn theme() -> UiTheme {
    UiTheme::new(&ReviewTheme::default())
}

fn search_picker(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut Window, &mut Context<SearchPicker>) -> SearchPicker + 'static,
) -> ViewHarness<SearchPicker, SearchPickerEvent> {
    search_picker_sized(cx, BASE_SIZE, build)
}

fn search_picker_sized(
    cx: &mut TestAppContext,
    interface_size: f32,
    build: impl FnOnce(&mut Window, &mut Context<SearchPicker>) -> SearchPicker + 'static,
) -> ViewHarness<SearchPicker, SearchPickerEvent> {
    cx.update(|cx| {
        gpui_base::init(cx);
        InterfaceSize::init(interface_size, cx);
    });
    let harness = ViewHarness::open(cx, WindowOptions::default(), |window, cx| cx.new(|cx| build(window, cx)));
    harness.draw(cx);
    harness
}

fn theme_picker(cx: &mut TestAppContext, themes: Arc<[ThemeChoice]>) -> ViewHarness<ThemePicker, ThemePickerEvent> {
    cx.update(gpui_base::init);
    let harness = ViewHarness::open(cx, WindowOptions::default(), |window, cx| {
        let current = ReviewTheme::default();
        cx.new(|cx| ThemePicker::new("themes", &current, themes, UiTheme::new(&current), window, cx))
    });
    harness.draw(cx);
    harness
}

fn theme_events(harness: &ViewHarness<ThemePicker, ThemePickerEvent>, cx: &TestAppContext) -> Vec<Event> {
    harness
        .events(cx)
        .into_iter()
        .map(|event| match event {
            ThemePickerEvent::Preview(theme) => Event::Preview(theme.id().clone()),
            ThemePickerEvent::Commit(theme) => Event::Commit(theme.id().clone()),
            ThemePickerEvent::Cancel(theme) => Event::Cancel(theme.id().clone()),
        })
        .collect()
}

fn position(themes: &[ThemeChoice], theme: &ReviewTheme) -> usize {
    themes.iter().position(|choice| choice.theme.id() == theme.id()).expect("the theme is listed")
}
