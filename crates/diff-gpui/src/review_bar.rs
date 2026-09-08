use crate::{
    Cancel, CopyReview, DiffViewer, ShowThemePicker, SubmitReview, ViewerPane,
    ui::prelude::{ActionBar, Button, ButtonVariant, ControlSize, MutedText},
};
use clankerdiff_core::{DiffReviewEvent, DiffScope};
use gpui::{Context, div, prelude::*};

impl DiffViewer {
    pub(crate) fn render_review_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.ui_theme();
        let hint = if self.comment_editor.is_some() {
            "Enter save · Shift-Enter newline · Esc cancel"
        } else if self.pane == ViewerPane::Files {
            "j/k entry · h/l fold/open · Tab pane · ? help"
        } else if self.layout().is_split() {
            "j/k line · ←/→ side · c comment · s submit · ? help"
        } else {
            "j/k line · c comment · e/x edit/delete · s submit · y copy · ? help"
        };
        let scope = self.scope();
        let pending = self.repository_pending();
        ActionBar::new(theme)
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(MutedText::new(
                        format!("{hint}    ·    {} review comments", self.review().len()),
                        theme,
                    )),
            )
            .child(
                Button::new("scope-unstaged", "Unstaged", theme)
                    .size(ControlSize::Small)
                    .selected(scope == DiffScope::Unstaged)
                    .disabled(pending)
                    .on_click(cx.listener(|viewer, _, window, cx| {
                        viewer.set_scope_intent(DiffScope::Unstaged, window, cx);
                    })),
            )
            .child(
                Button::new("scope-staged", "Staged", theme)
                    .size(ControlSize::Small)
                    .selected(scope == DiffScope::Staged)
                    .disabled(pending)
                    .on_click(cx.listener(|viewer, _, window, cx| {
                        viewer.set_scope_intent(DiffScope::Staged, window, cx);
                    })),
            )
            .child(
                Button::new("scope-both", "Both", theme)
                    .size(ControlSize::Small)
                    .selected(scope == DiffScope::Both)
                    .disabled(pending)
                    .on_click(cx.listener(|viewer, _, window, cx| {
                        viewer.set_scope_intent(DiffScope::Both, window, cx);
                    })),
            )
            .child(
                Button::new("select-theme", "Theme", theme)
                    .size(ControlSize::Small)
                    .on_click(cx.listener(|viewer, _, window, cx| {
                        viewer.show_theme_picker(&ShowThemePicker, window, cx);
                    })),
            )
            .when(!self.review().is_empty(), |bar| {
                bar.child(
                    Button::new("copy-review", "Copy", theme)
                        .variant(ButtonVariant::Secondary)
                        .size(ControlSize::Small)
                        .on_click(cx.listener(|viewer, _, window, cx| {
                            viewer.copy_review(&CopyReview, window, cx);
                        })),
                )
                .child(
                    Button::new("submit-review", "Submit", theme)
                        .variant(ButtonVariant::Primary)
                        .size(ControlSize::Small)
                        .on_click(cx.listener(|viewer, _, window, cx| {
                            viewer.submit_review(&SubmitReview, window, cx);
                        })),
                )
            })
            .child(
                Button::new("cancel-review", "Cancel", theme)
                    .size(ControlSize::Small)
                    .on_click(
                        cx.listener(|viewer, _, window, cx| viewer.cancel(&Cancel, window, cx)),
                    ),
            )
    }

    pub(crate) fn set_scope_intent(
        &mut self,
        scope: DiffScope,
        _: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        if self.repository_pending() {
            return;
        }
        cx.emit(DiffReviewEvent::SetScope(scope));
    }
}
