use clankerdiff_rouge_ui::{
    prelude::*,
    testing::{TestResult, component_test},
};
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{Key, Styled, div, show};
use rouge_reactive::signal;

use ConfirmDialogEvent::{Alternate, Cancelled, Confirmed};

#[test]
fn confirmation_handles_keyboard_buttons_and_backdrop_and_restores_focus() -> TestResult {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    let (mut ui, answer) = component_test().size(640.0, 420.0).build_with(|| {
        let theme = ActiveTheme::install(ReviewTheme::default()).ui();
        let answer = signal(None);
        let open = signal(false);
        let root = div()
            .size_full()
            .child(Button::new("open", "Open", theme).keyboard_focusable().on_click(move || open.set(true)))
            .child(show(
                move || open.get(),
                move || {
                    ConfirmDialog::new("dialog", "Delete?", "Cannot be undone", "Delete", theme, move |event| {
                        answer.set(Some(event));
                        open.set(false);
                    })
                    .alternate("Keep a copy")
                },
            ));
        (root, answer)
    });
    ui.click_on("open")?;
    ui.press(Key::Enter);
    assert_eq!(answer.get(), Some(Confirmed));
    ui.press(Key::Enter);
    assert!(ui.bounds_of("dialog").is_some());
    ui.click_on("confirm-alternate")?;
    assert_eq!(answer.get(), Some(Alternate));
    ui.press(Key::Enter);
    ui.click_on("dialog")?;
    assert!(ui.bounds_of("dialog").is_some());
    ui.click_at(5.0, 5.0);
    assert_eq!(answer.get(), Some(Cancelled));
    ui.press(Key::Enter);
    for _ in 0..4 {
        ui.press(Key::Tab);
    }
    answer.set(None);
    ui.press(Key::Enter);
    assert_eq!(answer.get(), Some(Cancelled));
    assert!(ui.bounds_of("dialog").is_none());
    ui.press(Key::Enter);
    ui.press(Key::Escape);
    assert!(ui.bounds_of("dialog").is_none());
    ui.press(Key::Enter);
    ui.click_on("confirm-accept")?;
    assert_eq!(answer.get(), Some(Confirmed));
    Ok(())
}

#[test]
fn escape_cancels_even_without_a_cancel_button() {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    let (mut ui, answer) = component_test().build_with(|| {
        let theme = ActiveTheme::install(ReviewTheme::default()).ui();
        let answer = signal(None);
        (
            ConfirmDialog::new("alert", "Alert", "Message", "OK", theme, move |event| answer.set(Some(event)))
                .without_cancel(),
            answer,
        )
    });
    assert!(ui.bounds_of("confirm-cancel").is_none());
    ui.press(Key::Escape);
    assert_eq!(answer.get(), Some(ConfirmDialogEvent::Cancelled));
}
