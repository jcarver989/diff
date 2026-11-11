use clankerdiff_rouge_ui::{
    interface_size::SizeAction,
    prelude::*,
    testing::{TestError, TestResult, component_test},
};
use rouge_core::{Key, Styled, div};

#[test]
fn mounted_rem_layout_follows_shortcuts_and_preview_transactions() -> TestResult {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    let (mut ui, size) = component_test().build_with(|| {
        let size = InterfaceSize::install(18.0);
        (size.bind(div().autofocus().child(div().test_id("ruler").w(rem(32.0)).h(rem(16.0)))), size)
    });
    let width = |ui: &mut clankerdiff_rouge_ui::testing::TestUi| {
        ui.advance(0.0);
        ui.bounds_of("ruler").map(|r| r.size.width).ok_or_else(|| TestError::NoTestId("ruler".into()))
    };
    assert!((width(&mut ui)? - 36.0).abs() < 0.1);
    ui.press_with(Key::Character("=".into()), key_binding("secondary-=")?.modifiers);
    assert!((width(&mut ui)? - 38.0).abs() < 0.1);
    size.preview(22.0);
    assert!((size.committed() - 19.0).abs() < 0.1);
    assert!((width(&mut ui)? - 44.0).abs() < 0.1);
    size.cancel();
    assert!((width(&mut ui)? - 38.0).abs() < 0.1);
    size.preview(100.0);
    size.commit();
    assert!((width(&mut ui)? - 64.0).abs() < 0.1);
    size.set(-100.0);
    assert!((width(&mut ui)? - 20.0).abs() < 0.1);
    size.apply(SizeAction::Reset);
    assert!((width(&mut ui)? - 36.0).abs() < 0.1);
    Ok(())
}
