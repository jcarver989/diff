use crate::{
    DiffViewer, ViewerPane,
    ui::prelude::{ActionBar, Button, ButtonVariant, ControlSize, EmptyState, MutedText, NoticeTone},
};
use clankerdiff_core::{DiffReviewCommand, ReviewCommand};
use gpui::{Context, div, prelude::*};

impl DiffViewer {
    pub(crate) fn render_review_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.ui_theme();
        let hint = if self.comment_editor.is_some() {
            "Enter save · Shift-Enter newline · Esc cancel"
        } else if self.session().source_view().is_some() {
            "j/k line · o back to diff · s submit · y copy · ? help"
        } else if self.pane == ViewerPane::Files {
            "j/k entry · h/l fold/open · Tab pane · ? help"
        } else if self.layout().is_split() {
            "j/k line · ←/→ side · c comment · s submit · ? help"
        } else {
            "j/k line · c comment · e/x edit/delete · s submit · y copy · ? help"
        };
        ActionBar::new(theme)
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(MutedText::new(format!("{hint}    ·    {} review comments", self.review().len()), theme)),
            )
            .when(self.options.themes, |bar| {
                bar.child(
                    Button::new("select-theme", "Theme", theme)
                        .disabled(!self.command_enabled(&ReviewCommand::OpenThemePicker.into()))
                        .size(ControlSize::Small)
                        .on_click(cx.listener(|viewer, _, window, cx| {
                            viewer.handle_command(ReviewCommand::OpenThemePicker, window, cx);
                        })),
                )
            })
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
            .child(Button::new("cancel-review", "Cancel", theme).size(ControlSize::Small).on_click(cx.listener(
                |viewer, _, window, cx| {
                    viewer.handle_command(ReviewCommand::Cancel, window, cx);
                },
            )))
    }

    pub(crate) fn render_clean(&self) -> impl IntoElement {
        div()
            .debug_selector(|| "changes-clean".to_owned())
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_4()
            .child(div().flex_none().child(EmptyState::new(
                "No changes",
                format!("Nothing to review in {}", self.session().target()),
                NoticeTone::Neutral,
                self.ui_theme(),
            )))
    }
}
