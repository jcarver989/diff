use crate::{
    DiffViewer, ViewerPane,
    ui::prelude::{
        ActionBar, Button, ButtonVariant, ControlSize, EmptyState, MutedText, NoticeTone, Typography, TypographyExt,
        rem,
    },
};
use clankerdiff_core::{DiffReviewCommand, DiffScope, ReviewCommand};
use gpui::{Context, div, prelude::*};

const MAX_COMMIT_TITLE_WIDTH: f32 = 480.0;

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
        let mut bar = ActionBar::new(theme).child(
            div()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(MutedText::new(format!("{hint}    ·    {} review comments", self.review().len()), theme)),
        );
        bar = bar.child(self.render_navigation(cx));
        bar.when(self.options.themes, |bar| {
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

    pub(crate) fn render_clean(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
            .child(self.render_navigation(cx))
    }

    fn render_navigation(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.ui_theme();
        let button = |id: &'static str, label: &'static str, command: DiffReviewCommand| {
            Button::new(id, label, theme).size(ControlSize::Small).disabled(!self.command_enabled(&command)).on_click(
                cx.listener(move |viewer, _, window, cx| {
                    viewer.handle_command(command.clone(), window, cx);
                }),
            )
        };
        let stepper = self.session().page_title().map(|title| {
            div()
                .debug_selector(|| "commit-stepper".to_owned())
                .min_w_0()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    button("commit-previous", "‹", DiffReviewCommand::StepTarget(-1))
                        .aria_label("Previous commit ([)")
                        .tooltip("Previous commit ([)"),
                )
                .child(
                    div()
                        .debug_selector(|| "commit-page".to_owned())
                        .min_w_0()
                        .max_w(rem(MAX_COMMIT_TITLE_WIDTH))
                        .truncate()
                        .typography(Typography::Small)
                        .text_color(theme.colors.text)
                        .child(title),
                )
                .child(
                    button("commit-next", "›", DiffReviewCommand::StepTarget(1))
                        .aria_label("Next commit (])")
                        .tooltip("Next commit (])"),
                )
        });
        let selected = self.session().target().scope();
        div().flex().items_center().gap_1().children(stepper).children(
            [
                ("scope-unstaged", "Unstaged", DiffScope::Unstaged),
                ("scope-staged", "Staged", DiffScope::Staged),
                ("scope-both", "Both", DiffScope::Both),
            ]
            .map(|(id, label, scope)| {
                button(id, label, DiffReviewCommand::SetScope(scope)).selected(selected == Some(scope))
            }),
        )
    }
}
