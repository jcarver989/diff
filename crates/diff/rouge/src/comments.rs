use crate::{ReviewHandle, comment_editor::composer};
use clankerdiff_core::{DiffSide, LineAnchor, ReviewCommand, ReviewComment};
use clankerdiff_rouge_ui::{
    components::IconButton,
    icon::IconName,
    theme::{UiTheme, color},
    tokens::{RADIUS_CONTROL, rem, scaled},
    typography::{Typography, TypographyExt},
};
use rouge_core::{Styled, View, div, dynamic, text};
use rouge_reactive::memo;

pub(crate) fn thread(
    handle: ReviewHandle,
    row: usize,
    side: DiffSide,
    anchor: LineAnchor,
    theme: UiTheme,
) -> impl View {
    let comments = memo({
        let handle = handle.clone();
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
    let anchor_title = anchor.clone();
    let draft = memo({
        let handle = handle.clone();
        move || handle.draft_placement().get().filter(|(location, _)| *location == anchor)
    });
    let palette = handle.theme();
    div()
        .min_w(0.0)
        .flex()
        .col()
        .gap(rem(8.0))
        .px(rem(12.0))
        .pt(rem(4.0))
        .pb(rem(12.0))
        .bg(move || palette.with(|t| color(t.diff.background)))
        .child(dynamic({
            let handle = handle.clone();
            move || {
                let comments = comments.get();
                if comments.is_empty() {
                    return div().into_any();
                }
                let last = comments.len() - 1;
                div()
                    .test_id(format!("comment-thread-{row}"))
                    .w_full()
                    .flex()
                    .col()
                    .rounded(scaled(RADIUS_CONTROL))
                    .border(1.0)
                    .border_color(theme.paint(|c| c.border))
                    .overflow_hidden()
                    .children(
                        comments
                            .into_iter()
                            .enumerate()
                            .map(|(index, comment)| card(&handle, row, side, comment, index == last, theme)),
                    )
                    .into_any()
            }
        }))
        .child(dynamic(move || {
            draft.get().map_or_else(
                || div().into_any(),
                |(_, editing)| {
                    let body = handle.with(|state| {
                        state.session().draft().map_or_else(String::new, |draft| draft.body().to_owned())
                    });
                    let line =
                        anchor_title.line_number().map_or_else(|| "line".to_owned(), |line| format!("line {line}"));
                    let side = if anchor_title.side == DiffSide::Old { "old" } else { "new" };
                    let verb = if editing.is_some() { "Edit comment" } else { "Add a comment" };
                    composer(
                        format!("{verb} on {side} {line}"),
                        body,
                        editing.is_some(),
                        theme,
                        {
                            let handle = handle.clone();
                            move |body| handle.set_draft_body(body)
                        },
                        {
                            let handle = handle.clone();
                            move |body| {
                                handle.set_draft_body(body);
                                handle.handle_command(ReviewCommand::SubmitComment);
                            }
                        },
                        {
                            let handle = handle.clone();
                            move || {
                                handle.handle_command(ReviewCommand::Cancel);
                            }
                        },
                    )
                    .into_any()
                },
            )
        }))
}

fn card(
    handle: &ReviewHandle,
    row: usize,
    side: DiffSide,
    comment: ReviewComment,
    last: bool,
    theme: UiTheme,
) -> impl View {
    let id = comment.id;
    let action = |label: &str, icon, hint: &str, edit| {
        let handle = handle.clone();
        IconButton::new(format!("{label}-comment-{id}"), icon, hint, theme).keyboard_focusable().on_click(move || {
            handle.comment_action(row, side, id, edit);
        })
    };
    let which = if comment.anchor.side == DiffSide::Old { "old" } else { "new" };
    let line = comment.anchor.line_number().map_or_else(|| "line".to_owned(), |line| format!("line {line}"));
    let card = div().test_id(format!("comment-{id}")).w_full().flex().col();
    let card = if last { card } else { card.border_b(1.0).border_color(theme.paint(|c| c.border)) };
    card.child(
        div()
            .px(rem(12.0))
            .py(rem(8.0))
            .bg(theme.paint(|c| c.surface_selected))
            .flex()
            .row()
            .items_center()
            .gap(rem(8.0))
            .child(
                div().flex_1().min_w(0.0).child(
                    text(format!("Your comment on {which} {line}"))
                        .typography(Typography::Label)
                        .color(theme.paint(|c| c.text_muted)),
                ),
            )
            .child(action("Edit", IconName::Pencil, "Edit comment", true))
            .child(action("Delete", IconName::X, "Delete comment", false)),
    )
    .child(
        div()
            .p(rem(12.0))
            .bg(theme.paint(|c| c.surface))
            .child(text(comment.body).typography(Typography::Body).color(theme.paint(|c| c.text))),
    )
}
