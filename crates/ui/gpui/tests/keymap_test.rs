use clankerdiff_gpui_ui::keymap::{binding_label, key_binding};
use gpui::{KeyBinding, NoAction, OperatingSystem, PlatformInfo, PlatformTarget, TestAppContext};

#[gpui::test]
fn secondary_is_command_on_mac(cx: &mut TestAppContext) {
    host(cx, OperatingSystem::Mac);
    let binding = cx.update(|cx| key_binding("secondary-shift-s", NoAction, None, cx));
    let modifiers = *binding.keystrokes()[0].modifiers();
    assert!(modifiers.platform && modifiers.shift && !modifiers.control);
    assert_eq!(label(cx, &binding), "⌘⇧S");
}

#[gpui::test]
fn secondary_is_control_on_linux(cx: &mut TestAppContext) {
    host(cx, OperatingSystem::Linux);
    let binding = cx.update(|cx| key_binding("secondary-shift-s", NoAction, None, cx));
    let modifiers = *binding.keystrokes()[0].modifiers();
    assert!(modifiers.control && modifiers.shift && !modifiers.platform);
    assert_eq!(label(cx, &binding), "ctrl-shift-S");
}

#[gpui::test]
fn a_context_limits_where_the_binding_applies(cx: &mut TestAppContext) {
    let binding = cx.update(|cx| key_binding("secondary-r", NoAction, Some("Editor && mode == browse"), cx));
    assert!(binding.predicate().is_some());
}

#[gpui::test]
#[should_panic(expected = "invalid key context `Editor &&`")]
fn invalid_contexts_panic(cx: &mut TestAppContext) {
    let _ = cx.update(|cx| key_binding("secondary-r", NoAction, Some("Editor &&"), cx));
}

fn host(cx: &mut TestAppContext, os: OperatingSystem) {
    cx.set_platform_info(PlatformInfo { os, target: PlatformTarget::Native });
}

fn label(cx: &mut TestAppContext, binding: &KeyBinding) -> String {
    cx.update(|cx| binding_label(binding, cx))
}
