use clankerdiff_gpui_ui::{
    InterfaceSize,
    interface_size::{
        DecreaseInterfaceSize, IncreaseInterfaceSize, MAX_INTERFACE_SIZE, MIN_INTERFACE_SIZE,
        ResetInterfaceSize,
    },
    testing::ComponentHarnessBuilder,
};
use gpui::{IntoElement, TestAppContext, div, px};

#[gpui::test]
fn the_interface_size_is_the_window_rem_size(cx: &mut TestAppContext) {
    ComponentHarnessBuilder {
        interface_size: 18.0,
        ..ComponentHarnessBuilder::default()
    }
    .run(cx, empty, |harness, cx| {
        assert_eq!(harness.rem_size(cx), px(18.0));
        cx.update(|cx| InterfaceSize::set(21.0, cx));
        assert_eq!(harness.rem_size(cx), px(21.0));
        Ok(())
    });
}

#[gpui::test]
fn actions_grow_shrink_and_reset_the_interface(cx: &mut TestAppContext) {
    ComponentHarnessBuilder::default().run(cx, empty, |harness, cx| {
        harness.dispatch_action(cx, &IncreaseInterfaceSize)?;
        harness.dispatch_action(cx, &IncreaseInterfaceSize)?;
        assert_eq!(harness.rem_size(cx), px(18.0));
        harness.dispatch_action(cx, &DecreaseInterfaceSize)?;
        assert_eq!(harness.rem_size(cx), px(17.0));
        harness.dispatch_action(cx, &ResetInterfaceSize)?;
        assert_eq!(harness.rem_size(cx), px(16.0));
        Ok(())
    });
}

#[gpui::test]
fn the_interface_size_stays_in_its_supported_range(cx: &mut TestAppContext) {
    ComponentHarnessBuilder::default().run(cx, empty, |harness, cx| {
        cx.update(|cx| InterfaceSize::set(1_000.0, cx));
        assert_eq!(harness.rem_size(cx), px(MAX_INTERFACE_SIZE));
        cx.update(|cx| InterfaceSize::set(1.0, cx));
        assert_eq!(harness.rem_size(cx), px(MIN_INTERFACE_SIZE));
        Ok(())
    });
}

#[gpui::test]
fn keyboard_shortcuts_change_the_interface_size(cx: &mut TestAppContext) {
    ComponentHarnessBuilder::default().run(cx, empty, |harness, cx| {
        harness.simulate_keystrokes(cx, "secondary-=");
        assert_eq!(harness.rem_size(cx), px(17.0));
        harness.simulate_keystrokes(cx, "secondary-0");
        assert_eq!(harness.rem_size(cx), px(16.0));
        Ok(())
    });
}

fn empty(_: &mut gpui::Window, _: &mut gpui::App) -> gpui::AnyElement {
    div().into_any_element()
}
