use clankerdiff_gpui_ui::{
    prelude::{ConfirmDialog, ConfirmDialogEvent, UiTheme},
    testing::ViewHarness,
};
use clankerdiff_theme::ReviewTheme;
use gpui::{AppContext as _, TestAppContext, WindowOptions};

#[gpui::test]
fn enter_confirms(cx: &mut TestAppContext) {
    let dialog = dialog(cx);

    dialog.simulate_keystrokes(cx, "enter");

    assert_eq!(dialog.events(cx), [ConfirmDialogEvent::Confirmed]);
}

#[gpui::test]
fn escape_cancels(cx: &mut TestAppContext) {
    let dialog = dialog(cx);

    dialog.simulate_keystrokes(cx, "escape");

    assert_eq!(dialog.events(cx), [ConfirmDialogEvent::Cancelled]);
}

#[gpui::test]
fn the_buttons_answer(cx: &mut TestAppContext) {
    let dialog = dialog(cx);

    dialog
        .click(cx, "close-confirm")
        .expect("the confirm button is drawn");
    dialog
        .click(cx, "close-cancel")
        .expect("the cancel button is drawn");

    assert_eq!(
        dialog.events(cx),
        [ConfirmDialogEvent::Confirmed, ConfirmDialogEvent::Cancelled]
    );
}

fn dialog(cx: &mut TestAppContext) -> ViewHarness<ConfirmDialog, ConfirmDialogEvent> {
    cx.update(ConfirmDialog::bind_keys);
    let theme = UiTheme::new(&ReviewTheme::default());
    let dialog = ViewHarness::open(cx, WindowOptions::default(), |window, cx| {
        cx.new(|cx| {
            ConfirmDialog::new(
                "close",
                "Close?",
                "It is running.",
                "Close",
                theme,
                window,
                cx,
            )
        })
    });
    dialog.draw(cx);
    dialog
}
