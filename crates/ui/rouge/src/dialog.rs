use crate::{
    components::{ButtonVariant, ModalSize, action_bar, button, modal},
    typography::UiStyles,
};
use rouge_core::{Key, View, component, div, text, view};
use rouge_reactive::Callback;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmDialogEvent {
    Confirmed,
    Alternate,
    Cancelled,
}

#[component]
pub fn confirm_dialog(
    #[prop(into)] id: String,
    #[prop(into)] title: String,
    #[prop(into)] message: String,
    #[prop(into)] confirm: String,
    confirm_variant: Option<ButtonVariant>,
    #[prop(into)] alternate: Option<String>,
    alternate_variant: Option<ButtonVariant>,
    #[prop(default)] without_cancel: bool,
    #[prop(into)] on_event: Callback<(ConfirmDialogEvent,)>,
) -> impl View {
    use ConfirmDialogEvent::{Alternate, Cancelled, Confirmed};
    let emit = move |event| move || on_event.call(event);
    let cancel = (!without_cancel).then(|| {
        view! { <button id="confirm-cancel" label="Cancel" keyboard_focusable on_click=emit(Cancelled)/> }
    });
    let alternate = alternate.map(|label| {
        view! {
            <button id="confirm-alternate" label variant={alternate_variant.unwrap_or(ButtonVariant::Secondary)}
                    keyboard_focusable on_click=emit(Alternate)/>
        }
    });
    view! {
        <modal id title size=ModalSize::Compact on_dismiss=emit(Cancelled)>
            <div autofocus tab_index=-1
                 on_key_down={move |event| {
                     if event.key == Key::Enter {
                         event.stop_propagation();
                         on_event.call(Confirmed);
                     }
                 }}>
                <text body primary>{message}</text>
                <action_bar>
                    {cancel}
                    {alternate}
                    <button id="confirm-accept" label=confirm
                            variant={confirm_variant.unwrap_or(ButtonVariant::Destructive)} keyboard_focusable
                            on_click=emit(Confirmed)/>
                </action_bar>
            </div>
        </modal>
    }
}
