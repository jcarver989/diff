use crate::{ReviewHandle, comment_editor::composer};
use clankerdiff_core::{DiffSide, LineAnchor, ReviewCommand, ReviewComment};
use rouge_core::{Color, Div, Role, Styled, View, div, dynamic, px, show, text, view};
use rouge_reactive::memo;
use rouge_widgets::{
    IconName, UiStyles, icon_button,
    tokens::{HAIRLINE, Radius, Space},
};

pub(crate) fn thread(handle: ReviewHandle, row: usize, side: DiffSide, anchor: LineAnchor) -> impl View {
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
    let adding =
        move || handle.draft_placement().with(|draft| matches!(draft, Some((location, None)) if *location == anchor));
    let palette = handle.theme();
    view! {
        <div min_w=0.0 flex col gap=Space::X2 px=Space::X3 pt=Space::X1 pb=Space::X3 bg={move || palette.with(|t| Color::from(t.diff.background))}>
            <dynamic>{
                let comments = comments.get();
                let last = comments.len().checked_sub(1)?;
                Some(view! {
                    <div test_id={format!("comment-thread-{row}")} w_full flex col rounded=Radius::Control
                         border=HAIRLINE border_color=Role::Border overflow_hidden>
                        {..comments
                            .into_iter()
                            .enumerate()
                            .map(|(index, comment)| card(&handle, row, side, comment, index == last))}
                    </div>
                })
            }</dynamic>
            <show when=adding>{editor(handle, Some(format!("Add a comment on {which} {line}")))}</show>
        </div>
    }
}

fn card(handle: &ReviewHandle, row: usize, side: DiffSide, comment: ReviewComment, last: bool) -> impl View {
    let (handle, id) = (*handle, comment.id);
    let act = move |edit| {
        move || {
            handle.comment_action(row, side, id, edit);
        }
    };
    let which = if comment.anchor.side == DiffSide::Old { "old" } else { "new" };
    let line = comment.anchor.line_number().map_or_else(|| "line".to_owned(), |line| format!("line {line}"));
    let editing = memo(move || {
        handle
            .draft_placement()
            .with(|draft| matches!(draft, Some((anchor, Some(editing))) if *anchor == comment.anchor && *editing == id))
    });
    view! {
        <div test_id={format!("comment-{id}")} w_full flex col when:border_b=(!last, px(1.0))
             when:border_color=(!last, Role::Border)>
            <div px=Space::X3 py=Space::X2 bg=Role::SurfaceSelected flex row items_center gap=Space::X2>
                <div flex_1 min_w=0.0>
                    <text label muted>{format!("Your comment on {which} {line}")}</text>
                </div>
                <show when={move || !editing.get()}>
                    <icon_button test_id={format!("Edit-comment-{id}")} icon=IconName::Pencil label="Edit comment"
                                 on_click=act(true)/>
                    <icon_button test_id={format!("Delete-comment-{id}")} icon=IconName::X label="Delete comment"
                                 on_click=act(false)/>
                </show>
            </div>
            <dynamic>{
                if editing.get() {
                    editor(handle, None)
                } else {
                    view! { <div p=Space::X3 bg=Role::Surface><text body primary>{comment.body.clone()}</text></div> }
                }
            }</dynamic>
        </div>
    }
}

fn editor(handle: ReviewHandle, title: Option<String>) -> Div {
    let body = handle.with(|state| state.session().draft().map_or_default(|draft| draft.body().to_owned()));
    composer(
        title,
        body,
        move |body| handle.set_draft_body(body),
        move |body| {
            handle.set_draft_body(body);
            handle.handle_command(ReviewCommand::SubmitComment);
        },
        move || {
            handle.handle_command(ReviewCommand::Cancel);
        },
    )
}
