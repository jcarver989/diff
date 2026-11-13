use crate::{ReviewHandle, comment_editor::composer};
use clankerdiff_core::{DiffSide, LineAnchor, ReviewCommand, ReviewComment};
use rouge_core::{Color, Styled, Theme, View, div, dynamic, px, text, view};
use rouge_reactive::memo;
use rouge_widgets::{IconName, UiStyles, icon_button, tokens::RADIUS_CONTROL};

pub(crate) fn thread(handle: ReviewHandle, row: usize, side: DiffSide, anchor: LineAnchor) -> impl View {
    let theme = Theme::current();
    let comments = memo({
        let anchor = anchor.clone();
        move || {
            handle.comments_revision().get();
            handle.with(|state| {
                state
                    .session()
                    .review()
                    .comments_for_anchor(&anchor)
                    .filter(|c| !c.outdated)
                    .cloned()
                    .collect::<Vec<_>>()
            })
        }
    });
    let line = anchor.line_number().map_or_else(|| "line".to_owned(), |line| format!("line {line}"));
    let which = if anchor.side == DiffSide::Old { "old" } else { "new" };
    let draft = memo(move || handle.draft_placement().get().filter(|(location, _)| *location == anchor));
    let palette = handle.theme();
    view! {
        <div min_w=0.0 flex col gap=8.0 px=12.0 pt=4.0 pb=12.0 bg={move || palette.with(|t| Color::from(t.diff.background))}>
            <dynamic>{
                let comments = comments.get();
                let last = comments.len().checked_sub(1)?;
                Some(view! {
                    <div test_id={format!("comment-thread-{row}")} w_full flex col rounded=RADIUS_CONTROL
                         border=px(1.0) border_color=theme.border overflow_hidden>
                        {..comments
                            .into_iter()
                            .enumerate()
                            .map(|(index, comment)| card(&handle, row, side, comment, index == last))}
                    </div>
                })
            }</dynamic>
            <dynamic>{
                draft.get().map(|(_, editing)| {
                    let body = handle
                        .with(|state| state.session().draft().map_or_else(String::new, |draft| draft.body().to_owned()));
                    let verb = if editing.is_some() { "Edit comment" } else { "Add a comment" };
                    composer(
                        format!("{verb} on {which} {line}"),
                        body,
                        editing.is_some(),
                        move |body| handle.set_draft_body(body),
                        move |body| {
                            handle.set_draft_body(body);
                            handle.handle_command(ReviewCommand::SubmitComment);
                        },
                        move || {
                            handle.handle_command(ReviewCommand::Cancel);
                        },
                    )
                })
            }</dynamic>
        </div>
    }
}

fn card(handle: &ReviewHandle, row: usize, side: DiffSide, comment: ReviewComment, last: bool) -> impl View {
    let theme = Theme::current();
    let (handle, id) = (*handle, comment.id);
    let act = move |edit| {
        move || {
            handle.comment_action(row, side, id, edit);
        }
    };
    let which = if comment.anchor.side == DiffSide::Old { "old" } else { "new" };
    let line = comment.anchor.line_number().map_or_else(|| "line".to_owned(), |line| format!("line {line}"));
    view! {
        <div test_id={format!("comment-{id}")} w_full flex col when:border_b=(!last, px(1.0))
             when:border_color=(!last, theme.border)>
            <div px=12.0 py=8.0 bg=theme.surface_selected flex row items_center gap=8.0>
                <div flex_1 min_w=0.0>
                    <text label muted>{format!("Your comment on {which} {line}")}</text>
                </div>
                <icon_button id={format!("Edit-comment-{id}")} icon=IconName::Pencil label="Edit comment"
                             keyboard_focusable on_click=act(true)/>
                <icon_button id={format!("Delete-comment-{id}")} icon=IconName::X label="Delete comment"
                             keyboard_focusable on_click=act(false)/>
            </div>
            <div p=12.0 bg=theme.surface>
                <text body primary>{comment.body}</text>
            </div>
        </div>
    }
}
