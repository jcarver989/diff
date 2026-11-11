use clankerdiff_rouge_ui::{
    prelude::*,
    testing::{TestError, TestResult, component_test},
};
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{Key, Styled, div, view};
use rouge_reactive::signal;

#[test]
fn menu_flips_and_clamps_at_window_edges_and_activates_by_keyboard() -> TestResult {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    let (mut ui, chosen) = component_test().size(320.0, 220.0).build_with(|| {
        let _ = ActiveTheme::install(ReviewTheme::default());
        let chosen = signal(0);
        let items = [
            MenuItem::new("Edit", move || chosen.set(1)),
            MenuItem::separator(),
            MenuItem::new("Delete", move || chosen.set(2)).destructive(),
        ];
        let root = view! {
            <div size_full flex col items_end justify_end>
                <with_menu items><button id="trigger" label="Menu"/></with_menu>
            </div>
        };
        (root, chosen)
    });
    assert!(ui.bounds_of("menu-item-Edit").is_none());
    ui.click_on("trigger")?;
    ui.run(0.2);
    let trigger = ui.center_of("trigger")?;
    let entry = ui.bounds_of("menu-item-Delete").ok_or_else(|| TestError::NoTestId("menu-item-Delete".into()))?;
    assert!(entry.max().y < trigger.y && entry.max().x <= 320.0, "{entry:?}");
    ui.press(Key::End);
    ui.press(Key::Enter);
    assert_eq!(chosen.get(), 2);
    ui.run(0.2);
    assert!(ui.bounds_of("menu-item-Delete").is_none());
    ui.press(Key::Enter);
    ui.run(0.2);
    ui.click_on("menu-item-Edit")?;
    assert_eq!(chosen.get(), 1);
    ui.run(0.2);
    ui.press(Key::Enter);
    ui.run(0.2);
    ui.click_at(3.0, 3.0);
    ui.run(0.2);
    assert!(ui.bounds_of("menu-item-Edit").is_none());
    Ok(())
}
