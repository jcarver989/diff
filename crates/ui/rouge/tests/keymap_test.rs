use clankerdiff_rouge_ui::{
    prelude::*,
    testing::{TestResult, component_test},
};
use rouge_core::{Key, div};
use rouge_reactive::signal;

#[test]
fn secondary_matches_platform_and_binding_is_scoped_to_its_subtree() -> TestResult {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    let chord = key_binding("secondary-k")?;
    assert_eq!(chord.modifiers.meta, cfg!(target_os = "macos"));
    assert_eq!(chord.modifiers.ctrl, !cfg!(target_os = "macos"));
    assert_eq!(binding_label(&chord), if cfg!(target_os = "macos") { "⌘K" } else { "Ctrl+K" });
    assert!(key_binding("nonsense-k").is_err());
    let (mut ui, invoked) = component_test().build_with(|| {
        let invoked = signal(0);
        let root = div()
            .child(
                div()
                    .bind_key(chord.clone(), ())
                    .on_action(move |(): &()| invoked.update(|n| *n += 1))
                    .child(div().autofocus()),
            )
            .child(div().focusable());
        (root, invoked)
    });
    ui.press_with(Key::Character("k".into()), chord.modifiers);
    ui.press(Key::Tab);
    ui.press_with(Key::Character("k".into()), chord.modifiers);
    assert_eq!(invoked.get(), 1);
    Ok(())
}
