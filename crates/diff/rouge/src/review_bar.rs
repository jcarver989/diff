use crate::ReviewHandle;
use clankerdiff_core::{DRAFT_HINT, DiffReviewCommand, InteractionPhase, KeyState, ReviewCommand, browse_hint};
use clankerdiff_rouge_ui::{
    components::{ButtonVariant, ControlSize, action_bar, button},
    typography::UiStyles,
};
use rouge_core::{Styled, View, div, dynamic, text, view};
use rouge_reactive::memo;

pub(crate) fn review_bar(handle: ReviewHandle) -> impl View {
    let status = handle.status();
    let comments = handle.comments_revision();
    let count = memo(move || {
        comments.get();
        handle.with(|s| s.session().review().len())
    });
    let selection = handle.selection();
    let definition = handle.definition_revision();
    let hint = move || {
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
    };
    let themes = handle.with(|s| s.options().themes);
    dynamic(move || {
        status.get();
        let button = |id: &'static str, label: &'static str, variant: ButtonVariant, command: DiffReviewCommand| {
            let enabled = handle.with(|s| s.command_enabled(&command));
            view! {
                <button id label size=ControlSize::Small variant disabled={!enabled}
                        on_click={move || {
                            handle.handle_command(command.clone());
                        }}/>
            }
        };
        let review = (count.get() > 0).then(|| {
            view! {
                {button("copy-review", "Copy", ButtonVariant::Secondary, DiffReviewCommand::CopyReview)}
                {button("submit-review", "Submit", ButtonVariant::Primary, DiffReviewCommand::SubmitReview)}
            }
        });
        view! {
            <action_bar>
                <div flex_1 min_w=0.0 overflow_hidden><text no_wrap caption muted>{hint}</text></div>
                {themes.then(|| {
                    button("select-theme", "Theme", ButtonVariant::Secondary, ReviewCommand::OpenThemePicker.into())
                })}
                {review}
                {button("cancel-review", "Cancel", ButtonVariant::Secondary, ReviewCommand::Cancel.into())}
            </action_bar>
        }
    })
}
