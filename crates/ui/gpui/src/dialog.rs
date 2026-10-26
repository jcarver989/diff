use super::{
    components::{Button, ButtonVariant, Modal, ModalSize},
    theme::UiTheme,
};
use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, KeyBinding, SharedString, Window, actions, div, prelude::*,
};

actions!(confirm_dialog, [Confirm, Cancel]);

pub const CONTEXT: &str = "ConfirmDialog";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmDialogEvent {
    Confirmed,
    Alternate,
    Cancelled,
}

pub struct ConfirmDialog {
    id: SharedString,
    title: SharedString,
    message: SharedString,
    confirm: SharedString,
    confirm_variant: ButtonVariant,
    alternate: Option<SharedString>,
    alternate_variant: ButtonVariant,
    cancel: bool,
    theme: UiTheme,
    focus_handle: FocusHandle,
}

impl ConfirmDialog {
    pub fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("enter", Confirm, Some(CONTEXT)),
            KeyBinding::new("escape", Cancel, Some(CONTEXT)),
        ]);
    }

    pub fn new(
        id: impl Into<SharedString>,
        title: impl Into<SharedString>,
        message: impl Into<SharedString>,
        confirm: impl Into<SharedString>,
        theme: UiTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            id: id.into(),
            title: title.into(),
            message: message.into(),
            confirm: confirm.into(),
            confirm_variant: ButtonVariant::Destructive,
            alternate: None,
            alternate_variant: ButtonVariant::Secondary,
            cancel: true,
            theme,
            focus_handle,
        }
    }

    #[must_use]
    pub const fn confirm_variant(mut self, variant: ButtonVariant) -> Self {
        self.confirm_variant = variant;
        self
    }

    #[must_use]
    pub fn alternate(mut self, label: impl Into<SharedString>) -> Self {
        self.alternate = Some(label.into());
        self
    }

    #[must_use]
    pub const fn alternate_variant(mut self, variant: ButtonVariant) -> Self {
        self.alternate_variant = variant;
        self
    }

    #[must_use]
    pub const fn without_cancel(mut self) -> Self {
        self.cancel = false;
        self
    }
}

impl EventEmitter<ConfirmDialogEvent> for ConfirmDialog {}

impl Focusable for ConfirmDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ConfirmDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dialog = cx.entity().downgrade();
        let emit = |event| cx.listener(move |_, _, _, cx| cx.emit(event));
        Modal::new(self.id.clone(), self.title.clone(), self.theme)
            .size(ModalSize::Compact)
            .on_dismiss(move |_, cx| {
                let _ = dialog.update(cx, |_, cx| cx.emit(ConfirmDialogEvent::Cancelled));
            })
            .child(
                div()
                    .key_context(CONTEXT)
                    .track_focus(&self.focus_handle)
                    .on_action(cx.listener(|_, _: &Confirm, _, cx| {
                        cx.emit(ConfirmDialogEvent::Confirmed);
                    }))
                    .on_action(cx.listener(|_, _: &Cancel, _, cx| {
                        cx.emit(ConfirmDialogEvent::Cancelled);
                    }))
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(div().text_color(self.theme.colors.text_secondary).child(self.message.clone()))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .children(self.cancel.then(|| {
                                Button::new(format!("{}-cancel", self.id), "Cancel", self.theme)
                                    .variant(ButtonVariant::Secondary)
                                    .on_click(emit(ConfirmDialogEvent::Cancelled))
                            }))
                            .children(self.alternate.clone().map(|alternate| {
                                Button::new(format!("{}-alternate", self.id), alternate, self.theme)
                                    .variant(self.alternate_variant)
                                    .on_click(emit(ConfirmDialogEvent::Alternate))
                            }))
                            .child(
                                Button::new(format!("{}-confirm", self.id), self.confirm.clone(), self.theme)
                                    .variant(self.confirm_variant)
                                    .on_click(emit(ConfirmDialogEvent::Confirmed)),
                            ),
                    ),
            )
    }
}
