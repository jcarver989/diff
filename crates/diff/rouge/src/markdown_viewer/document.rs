use super::{
    MarkdownHandle,
    content::{Paint, Row},
};
use crate::{comment_editor::composer, ui::prelude::*, ui::tokens::RADIUS_CONTROL, ui::tokens::scaled};
use clankerdiff_core::ReviewCommand;
use clankerdiff_markdown::{MarkdownFocusPane, MarkdownReviewComment, MarkdownTargetId};
use rouge_core::{AnyView, Cursor, Div, RowKey, Styled, View, div, dynamic, root_font_size, show, text, virtual_list};
use rouge_reactive::{Memo, Signal, memo, signal};

const COLUMN_WIDTH: f32 = 780.0;
const GUTTER_WIDTH: f32 = 28.0;
const ADD_BUTTON_SIZE: f32 = 18.0;
const TOP_PADDING: f32 = 32.0;
const BOTTOM_PADDING: f32 = 96.0;

pub(super) fn document(handle: &MarkdownHandle, paint: Paint) -> impl View {
    let rows = handle.rows;
    let revision = handle.revision;
    let theme = paint.ui;
    let list = virtual_list(move || rows.with(|rows| rows.len()), scaled(48.0), {
        let handle = handle.clone();
        move |index| row(&handle, index, &paint)
    })
    .row_identity(move |index| RowKey::new(rows.with_untracked(|r| r.key(index)), revision.get()))
    .measure_rows(|| u64::from(root_font_size().get().to_bits()))
    .handle(handle.list)
    .size_full();
    div()
        .test_id("markdown-document")
        .flex_1()
        .min_w(0.0)
        .min_h(0.0)
        .relative()
        .child(list)
        .child(div().absolute().right(0.0).top(0.0).bottom(0.0).child(ListScrollbar::list(handle.list)))
        .child(show(
            move || rows.with(|rows| rows.len() == 0),
            move || {
                div()
                    .test_id("markdown-empty")
                    .absolute()
                    .inset(0.0)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(EmptyState::new(IconName::FileText, "Nothing to review", "This document is empty.", theme))
            },
        ))
}

fn row(handle: &MarkdownHandle, index: usize, paint: &Paint) -> AnyView {
    let rows = handle.rows.get_untracked();
    let Some(row) = rows.get(index) else {
        return div().into_any();
    };
    let theme = paint.ui;
    let target = row.target;
    let active = handle.selected;
    let selected = memo(move || target.is_some() && active.get() == target);
    let hovered = signal(false);
    let block_hovered = signal(false);
    let base = row.background(paint);
    let mut block = row
        .block(paint)
        .on_hover(move |inside| {
            block_hovered.set_if_changed(inside);
        })
        .bg(move || {
            if selected.get() {
                theme.colors().surface_selected
            } else if target.is_some() && block_hovered.get() {
                theme.colors().surface_hover
            } else {
                base()
            }
        });
    let mut column = div().flex_1().min_w(0.0).pl(rem(row.indent())).flex().col();
    if let Some(target) = target {
        let select = handle.clone();
        block = block.cursor(Cursor::Pointer).on_click(move |_| select.select(target, MarkdownFocusPane::Document));
        column = column.child(block).child(thread(handle, target, theme)).child(draft(handle, target, theme));
    } else {
        column = column.child(block);
    }
    let top = if index == 0 { TOP_PADDING } else { row.gap };
    let bottom = if index + 1 == rows.len() { BOTTOM_PADDING } else { 0.0 };
    div()
        .test_id(format!("markdown-row-{index}"))
        .w_full()
        .px(rem(24.0))
        .pt(rem(top))
        .pb(rem(bottom))
        .flex()
        .row()
        .justify_center()
        .child(
            div()
                .w_full()
                .max_w(rem(COLUMN_WIDTH))
                .flex()
                .row()
                .on_hover(move |inside| {
                    hovered.set_if_changed(inside);
                })
                .child(gutter(handle, row, selected, hovered, theme))
                .child(column),
        )
        .into_any()
}

fn gutter(handle: &MarkdownHandle, row: &Row, selected: Memo<bool>, hovered: Signal<bool>, theme: UiTheme) -> Div {
    let gutter = div().w(rem(GUTTER_WIDTH)).flex_none().relative().child(show(
        move || selected.get(),
        move || div().absolute().top(0.0).bottom(0.0).right(rem(3.0)).w(2.0).rounded(1.0).bg(theme.paint(|c| c.accent)),
    ));
    let Some(target) = row.target else {
        return gutter;
    };
    let offset = (row.first_line_center() - ADD_BUTTON_SIZE / 2.0).max(1.0);
    let handle = handle.clone();
    gutter.child(show(move || hovered.get(), move || add_comment(handle.clone(), target, offset)))
}

fn add_comment(handle: MarkdownHandle, target: MarkdownTargetId, offset: f32) -> impl View {
    let palette = handle.theme;
    div()
        .test_id(format!("markdown-add-comment-{}", target.index()))
        .absolute()
        .left(rem(4.0))
        .top(rem(offset))
        .w(rem(ADD_BUTTON_SIZE))
        .h(rem(ADD_BUTTON_SIZE))
        .rounded(scaled(ADD_BUTTON_SIZE / 2.0))
        .bg(move || palette.with(|theme| color(theme.diff.accent)))
        .cursor(Cursor::Pointer)
        .flex()
        .items_center()
        .justify_center()
        .child(Icon::new(IconName::Plus, move || palette.with(|theme| color(theme.diff.background))).size(12.0))
        .on_click(move |event| {
            event.stop_propagation();
            handle.begin_comment_on(target);
        })
}

fn thread(handle: &MarkdownHandle, target: MarkdownTargetId, theme: UiTheme) -> impl View {
    let annotations = handle.annotations;
    let handle = handle.clone();
    dynamic(move || {
        annotations.get();
        let comments = handle.with(|session| {
            session.review().comments_for_target(session.document(), target).cloned().collect::<Vec<_>>()
        });
        if comments.is_empty() {
            return div().into_any();
        }
        let last = comments.len() - 1;
        div()
            .pt(rem(8.0))
            .pb(rem(4.0))
            .child(
                div()
                    .test_id(format!("markdown-comment-thread-{}", target.index()))
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
                            .map(|(index, comment)| card(&handle, target, comment, index == last, theme)),
                    ),
            )
            .into_any()
    })
}

fn card(
    handle: &MarkdownHandle,
    target: MarkdownTargetId,
    comment: MarkdownReviewComment,
    last: bool,
    theme: UiTheme,
) -> impl View {
    let id = comment.id;
    let edit = handle.clone();
    let delete = handle.clone();
    let title = if comment.outdated { "Outdated comment" } else { "Your comment" };
    let card = div().test_id(format!("markdown-comment-{id}")).w_full().flex().col();
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
                div()
                    .flex_1()
                    .min_w(0.0)
                    .child(text(title).typography(Typography::Label).color(theme.paint(|c| c.text_muted))),
            )
            .child(
                IconButton::new(format!("Edit-comment-{id}"), IconName::Pencil, "Edit comment", theme)
                    .keyboard_focusable()
                    .on_click(move || edit.edit_comment(target, id)),
            )
            .child(
                IconButton::new(format!("Delete-comment-{id}"), IconName::X, "Delete comment", theme)
                    .keyboard_focusable()
                    .on_click(move || delete.delete_comment(id)),
            ),
    )
    .child(
        div()
            .p(rem(12.0))
            .bg(theme.paint(|c| c.surface))
            .child(text(comment.body).typography(Typography::Body).color(theme.paint(|c| c.text))),
    )
}

fn draft(handle: &MarkdownHandle, target: MarkdownTargetId, theme: UiTheme) -> impl View {
    let draft = handle.draft;
    let handle = handle.clone();
    dynamic(move || {
        let Some((_, editing)) = draft.get().filter(|(id, _)| *id == target) else {
            return div().into_any();
        };
        let body = handle.with(|session| session.draft().map(|draft| draft.body().to_owned()).unwrap_or_default());
        let (change, submit, cancel) = (handle.clone(), handle.clone(), handle.clone());
        div()
            .pt(rem(8.0))
            .pb(rem(4.0))
            .child(composer(
                if editing.is_some() { "Edit comment" } else { "Add a comment" }.to_owned(),
                body,
                editing.is_some(),
                theme,
                move |body| change.set_draft_body(body),
                move |body| {
                    submit.set_draft_body(body.to_owned());
                    submit.run(ReviewCommand::SubmitComment);
                },
                move || cancel.run(ReviewCommand::Cancel),
            ))
            .into_any()
    })
}
