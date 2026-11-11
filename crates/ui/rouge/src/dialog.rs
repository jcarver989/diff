use crate::{
    components::{ActionBar, Button, ButtonVariant, Modal, ModalSize},
    theme::UiTheme,
    typography::{Typography, TypographyExt},
};
use rouge_core::{BuildCx, ElementId, Key, View, div, text};
use std::rc::Rc;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmDialogEvent {
    Confirmed,
    Alternate,
    Cancelled,
}
pub struct ConfirmDialog {
    id: String,
    title: String,
    message: String,
    confirm: String,
    theme: UiTheme,
    confirm_variant: ButtonVariant,
    alternate: Option<String>,
    alternate_variant: ButtonVariant,
    cancel: bool,
    emit: Rc<dyn Fn(ConfirmDialogEvent)>,
}
impl ConfirmDialog {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        message: impl Into<String>,
        confirm: impl Into<String>,
        theme: UiTheme,
        emit: impl Fn(ConfirmDialogEvent) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            message: message.into(),
            confirm: confirm.into(),
            theme,
            confirm_variant: ButtonVariant::Destructive,
            alternate: None,
            alternate_variant: ButtonVariant::Secondary,
            cancel: true,
            emit: Rc::new(emit),
        }
    }
    #[must_use]
    pub fn confirm_variant(mut self, variant: ButtonVariant) -> Self {
        self.confirm_variant = variant;
        self
    }
    #[must_use]
    pub fn alternate(mut self, label: impl Into<String>) -> Self {
        self.alternate = Some(label.into());
        self
    }
    #[must_use]
    pub fn alternate_variant(mut self, variant: ButtonVariant) -> Self {
        self.alternate_variant = variant;
        self
    }
    #[must_use]
    pub fn without_cancel(mut self) -> Self {
        self.cancel = false;
        self
    }
}
impl View for ConfirmDialog {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        use ConfirmDialogEvent::{Alternate, Cancelled, Confirmed};
        let emit = self.emit;
        let click = |event| {
            let emit = emit.clone();
            move || emit(event)
        };
        let mut actions = ActionBar::new(self.theme);
        if self.cancel {
            actions = actions.child(
                Button::new("confirm-cancel", "Cancel", self.theme).keyboard_focusable().on_click(click(Cancelled)),
            );
        }
        if let Some(alt) = self.alternate {
            actions = actions.child(
                Button::new("confirm-alternate", alt, self.theme)
                    .variant(self.alternate_variant)
                    .keyboard_focusable()
                    .on_click(click(Alternate)),
            );
        }
        let mut modal =
            Modal::new(self.id, self.title, self.theme).size(ModalSize::Compact).on_dismiss(click(Cancelled));
        actions = actions.child(
            Button::new("confirm-accept", self.confirm, self.theme)
                .variant(self.confirm_variant)
                .keyboard_focusable()
                .on_click(click(Confirmed)),
        );
        modal = modal.child(
            div()
                .autofocus()
                .tab_index(-1)
                .on_key_down(move |event| {
                    if event.key == Key::Enter {
                        event.stop_propagation();
                        emit(Confirmed);
                    }
                })
                .child(text(self.message).typography(Typography::Body).color(self.theme.paint(|c| c.text)))
                .child(actions),
        );
        modal.build(cx)
    }
}
