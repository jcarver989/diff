use clankerdiff_rouge_ui::{prelude::*, testing::component_test};
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::Key;
use rouge_reactive::signal;

#[test]
fn autofocus_selects_existing_value_and_enter_submits_replacement() {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    let (mut ui, submitted) = component_test().build_with(|| {
        let value = InlineEdit::input("original");
        let submitted = signal(String::new());
        let theme = ActiveTheme::install(ReviewTheme::default()).ui();
        (InlineEdit::new(value, theme).on_commit(move |text| submitted.set(text.to_owned())), submitted)
    });
    ui.type_text("replacement");
    ui.press(Key::Enter);
    assert_eq!(submitted.get(), "replacement");
}

#[test]
fn escape_cancels() {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    let (mut ui, cancelled) = component_test().build_with(|| {
        let cancelled = signal(false);
        let theme = ActiveTheme::install(ReviewTheme::default()).ui();
        (InlineEdit::new(InlineEdit::input("value"), theme).on_cancel(move || cancelled.set(true)), cancelled)
    });
    ui.press(Key::Escape);
    assert!(cancelled.get());
}
