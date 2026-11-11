use crate::ReviewHandle;
use clankerdiff_core::{DRAFT_HINT, DiffReviewCommand, InteractionPhase, KeyState, ReviewCommand, browse_hint};
use clankerdiff_rouge_ui::{
    components::{ActionBar, Button, ButtonVariant, ControlSize},
    theme::UiTheme,
    typography::{Typography, TypographyExt},
};
use rouge_core::{Styled, View, div, dynamic, text};
use rouge_reactive::memo;

pub(crate) fn review_bar(handle: ReviewHandle, theme: UiTheme) -> impl View {
    let status = handle.status();
    let comments = handle.comments_revision();
    let count = memo({
        let handle = handle.clone();
        move || {
            comments.get();
            handle.with(|s| s.session().review().len())
        }
    });
    let hint = {
        let handle = handle.clone();
        let selection = handle.selection();
        let definition = handle.definition_revision();
        move || {
            selection.get();
            definition.get();
            let hint = if status.get().context.phase == InteractionPhase::Draft {
                DRAFT_HINT
            } else {
                handle.with(|s| {
                    browse_hint(
                        KeyState::new(s.selection().pane, s.session(), s.definition_card().is_some()),
                        s.session().shows_source(),
                    )
                })
            };
            format!("{hint}    ·    {} review comments", count.get())
        }
    };
    let themes = handle.with(|s| s.options().themes);
    dynamic(move || {
        status.get();
        let button = |id, label, variant, command: DiffReviewCommand| {
            let enabled = handle.with(|s| s.command_enabled(&command));
            let handle = handle.clone();
            Button::new(id, label, theme).size(ControlSize::Small).variant(variant).disabled(!enabled).on_click(
                move || {
                    handle.handle_command(command.clone());
                },
            )
        };
        let mut bar =
            ActionBar::new(theme).child(div().flex_1().min_w(0.0).overflow_hidden().child(
                text(hint.clone()).no_wrap().typography(Typography::Caption).color(theme.paint(|c| c.text_muted)),
            ));
        if themes {
            bar = bar.child(button(
                "select-theme",
                "Theme",
                ButtonVariant::Secondary,
                ReviewCommand::OpenThemePicker.into(),
            ));
        }
        if count.get() > 0 {
            bar = bar
                .child(button("copy-review", "Copy", ButtonVariant::Secondary, DiffReviewCommand::CopyReview))
                .child(button("submit-review", "Submit", ButtonVariant::Primary, DiffReviewCommand::SubmitReview));
        }
        bar.child(button("cancel-review", "Cancel", ButtonVariant::Secondary, ReviewCommand::Cancel.into()))
    })
}
