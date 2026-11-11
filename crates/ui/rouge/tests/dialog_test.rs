use clankerdiff_rouge_ui::{
    prelude::*,
    testing::{TestResult, component_test},
};
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{Key, Styled, div, show, view};
use rouge_reactive::signal;

use ConfirmDialogEvent::{Alternate, Cancelled, Confirmed};

#[test]
fn confirmation_handles_keyboard_buttons_and_backdrop_and_restores_focus() -> TestResult {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    let (mut ui, answer) = component_test().size(640.0, 420.0).build_with(|| {
        let _ = ActiveTheme::install(ReviewTheme::default());
        let answer = signal(None);
        let open = signal(false);
        let root = view! {
            <div size_full>
                <button id="open" label="Open" keyboard_focusable on_click={move || open.set(true)}/>
                <show when={move || open.get()}>
                    <confirm_dialog id="dialog" title="Delete?" message="Cannot be undone" confirm="Delete"
                                    alternate="Keep a copy"
                                    on_event={move |event: ConfirmDialogEvent| {
                                        answer.set(Some(event));
                                        open.set(false);
                                    }}/>
                </show>
            </div>
        };
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
        let _ = ActiveTheme::install(ReviewTheme::default());
        let answer = signal(None);
        let dialog = view! {
            <confirm_dialog id="alert" title="Alert" message="Message" confirm="OK" without_cancel
                            on_event={move |event: ConfirmDialogEvent| answer.set(Some(event))}/>
        };
        (dialog, answer)
    });
    assert!(ui.bounds_of("confirm-cancel").is_none());
    ui.press(Key::Escape);
    assert_eq!(answer.get(), Some(ConfirmDialogEvent::Cancelled));
}
