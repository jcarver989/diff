use clankerdiff_rouge_ui::{
    prelude::*,
    testing::{TestError, TestResult, component_test},
};
use clankerdiff_ui_core::{ReviewTheme, ThemeChoice};
use rouge_core::{Key, Length, View, div, text};
use rouge_reactive::signal;
use std::sync::Arc;

use SearchPickerEvent::{Chose, Highlighted};

#[test]
fn search_filters_details_and_skips_disabled_rows() -> TestResult {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    let (mut ui, events) = component_test().size(780.0, 360.0).build_with(|| {
        let theme = ActiveTheme::install(ReviewTheme::default()).ui();
        let events = signal(Vec::new());
        let picker = SearchPicker::new(
            "picker",
            vec![
                PickerItem::new("alpha", "Alpha"),
                PickerItem::new("disabled", "Unavailable").disabled(true),
                PickerItem::new("beta", "Beta").subtitle("Nimbus").detail(PickerDetail::new("revision 42").code()),
            ],
            theme,
            move |event| events.update(|events| events.push(event)),
        )
        .searchable("Find")
        .preview(|index| div().test_id(format!("preview-{index}")).child(text(format!("Preview {index}"))).into_any());
        (picker, events)
    });
    ui.click_on("disabled")?;
    assert!(events.with(Vec::is_empty));
    ui.click_on("picker-query")?;
    ui.press(Key::ArrowDown);
    assert_eq!(events.get(), vec![Highlighted(2)]);
    assert!(ui.center_of("preview-2")?.x > ui.center_of("picker-results")?.x);
    ui.hover_on("alpha")?;
    assert!(ui.bounds_of("preview-0").is_some());
    ui.type_text("revision 42");
    assert!(ui.bounds_of("alpha").is_none());
    ui.press(Key::Enter);
    assert_eq!(events.with(|events| events.last().cloned()), Some(Chose(2)));
    Ok(())
}

#[test]
fn keyboard_highlight_reveals_the_entire_variable_height_row() -> TestResult {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    let mut ui = component_test().size(600.0, 460.0).build(|| {
        let theme = ActiveTheme::install(ReviewTheme::default()).ui();
        SearchPicker::new(
            "picker",
            (0..24)
                .map(|i| {
                    PickerItem::new(format!("option-{i}"), format!("Item {i}"))
                        .subtitle("Secondary text")
                        .group("Items")
                })
                .collect(),
            theme,
            |_| {},
        )
        .searchable("Find")
        .max_list_height(Length::Px(104.0))
    });
    ui.press(Key::PageDown);
    ui.press(Key::PageDown);
    ui.run(0.15);
    let bounds = |ui: &mut clankerdiff_rouge_ui::testing::TestUi, id: &str| {
        ui.bounds_of(id).ok_or_else(|| TestError::NoTestId(id.into()))
    };
    let viewport = bounds(&mut ui, "picker-results")?;
    let row = bounds(&mut ui, "option-16")?;
    assert!(
        row.origin.y >= viewport.origin.y - 0.5 && row.max().y <= viewport.max().y + 0.5,
        "row {row:?}, viewport {viewport:?}"
    );
    assert!(row.size.height > 28.0);
    Ok(())
}

#[test]
fn theme_picker_previews_commits_and_cancels_against_original_theme() -> TestResult {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    let original = ReviewTheme::default();
    let other = ReviewTheme::builtin("ayu")?;
    for accept in [true, false] {
        let themes: Arc<[_]> =
            [ThemeChoice::new("Original", original.clone()), ThemeChoice::new("Other", other.clone())].into();
        let baseline = original.clone();
        let (mut ui, (active, events)) = component_test().size(720.0, 400.0).build_with(move || {
            let active = ActiveTheme::install(baseline.clone());
            let events = signal(Vec::new());
            let picker = ThemePicker::new("themes", &baseline, themes, active.ui(), move |event| {
                let (name, theme) = match event {
                    ThemePickerEvent::Preview(theme) => ("preview", theme),
                    ThemePickerEvent::Commit(theme) => ("commit", theme),
                    ThemePickerEvent::Cancel(theme) => ("cancel", theme),
                };
                events.update(|events| events.push(name));
                active.set(theme);
            });
            (ThemePickerOverlay::new(picker, active.ui()), (active, events))
        });
        ui.type_text("Other");
        assert_eq!(active.review().revision(), other.revision());
        ui.press(if accept { Key::Enter } else { Key::Escape });
        assert_eq!(events.get(), vec!["preview", if accept { "commit" } else { "cancel" }]);
        assert_eq!(active.review().revision(), if accept { other.revision() } else { original.revision() });
    }
    Ok(())
}

#[test]
fn grouped_list_aligns_marker_prefix_label_and_check_columns() -> TestResult {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    let (mut ui, selected) = component_test().size(500.0, 300.0).build_with(|| {
        let theme = ActiveTheme::install(ReviewTheme::default()).ui();
        let selected = signal(None);
        let picker = PickerList::new("list", theme)
            .items([
                PickerItem::new("first", "Alpha")
                    .group("Group A")
                    .prefix("0123456")
                    .marker(PickerMarker::Pending)
                    .checked(true),
                PickerItem::new("middle", "Beta").group("Group B").prefix("1"),
                PickerItem::new("last", "Gamma").group("Group A").subtitle("Description"),
            ])
            .on_choose(move |index| selected.set(Some(index)));
        (picker, selected)
    });
    let first = ui.center_of("first-marker")?;
    let last = ui.center_of("last-marker")?;
    assert!((first.x - last.x).abs() < 0.1);
    let first = ui.center_of("first-label")?;
    let last = ui.center_of("last-label")?;
    assert!((first.x - last.x).abs() < 0.1);
    assert!(first.x < ui.center_of("first-check")?.x);
    assert!(first.y < last.y && last.y < ui.center_of("middle-label")?.y);
    assert!(ui.center_of("last-subtitle")?.y > last.y);
    ui.click_on("last")?;
    assert_eq!(selected.get(), Some(1));
    Ok(())
}
