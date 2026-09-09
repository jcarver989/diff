use crate::{
    DiffViewer, ViewerPane,
    ui::prelude::{ActionBar, Button, ButtonVariant, ControlSize, MutedText},
};
use clankerdiff_core::{DiffReviewCommand, DiffScope, ReviewCommand};
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
        let mut bar = ActionBar::new(theme).child(
            div()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(MutedText::new(
                    format!("{hint}    ·    {} review comments", self.review().len()),
                    theme,
                )),
        );
        for (id, label, value) in [
            ("scope-unstaged", "Unstaged", DiffScope::Unstaged),
            ("scope-staged", "Staged", DiffScope::Staged),
            ("scope-both", "Both", DiffScope::Both),
        ] {
            bar = bar.child(
                Button::new(id, label, theme)
                    .size(ControlSize::Small)
                    .selected(scope == value)
                    .disabled(!self.command_enabled(&DiffReviewCommand::SetScope(value)))
                    .on_click(cx.listener(move |viewer, _, window, cx| {
                        viewer.handle_command(DiffReviewCommand::SetScope(value), window, cx);
                    })),
            );
        }
        bar.child(
            Button::new("select-theme", "Theme", theme)
                .disabled(!self.command_enabled(&ReviewCommand::OpenThemePicker.into()))
                .size(ControlSize::Small)
                .on_click(cx.listener(|viewer, _, window, cx| {
                    viewer.handle_command(ReviewCommand::OpenThemePicker, window, cx);
                })),
        )
        .when(!self.review().is_empty(), |bar| {
            bar.child(
                Button::new("copy-review", "Copy", theme)
                    .disabled(!self.command_enabled(&DiffReviewCommand::CopyReview))
                    .variant(ButtonVariant::Secondary)
                    .size(ControlSize::Small)
                    .on_click(cx.listener(|viewer, _, window, cx| {
                        viewer.handle_command(DiffReviewCommand::CopyReview, window, cx);
                    })),
            )
            .child(
                Button::new("submit-review", "Submit", theme)
                    .disabled(!self.command_enabled(&DiffReviewCommand::SubmitReview))
                    .variant(ButtonVariant::Primary)
                    .size(ControlSize::Small)
                    .on_click(cx.listener(|viewer, _, window, cx| {
                        viewer.handle_command(DiffReviewCommand::SubmitReview, window, cx);
                    })),
            )
        })
        .child(
            Button::new("cancel-review", "Cancel", theme)
                .size(ControlSize::Small)
                .on_click(cx.listener(|viewer, _, window, cx| {
                    viewer.handle_command(ReviewCommand::Cancel, window, cx);
                })),
        )
    }
}
