use super::{
    MarkdownHandle,
    content::{Paint, Row},
};
use crate::{
    comment_editor::composer,
    ui::{prelude::*, tokens::RADIUS_CONTROL},
};
use clankerdiff_core::ReviewCommand;
use clankerdiff_markdown::{MarkdownFocusPane, MarkdownReviewComment, MarkdownTargetId};
use rouge_core::{
    AnyView, Apply, Cursor, Div, RowKey, Styled, View, component, div, dynamic, px, root_font_size, show, text, view,
    virtual_list,
};
use rouge_reactive::{Memo, Signal, memo, signal};

const COLUMN_WIDTH: f32 = 780.0;
const GUTTER_WIDTH: f32 = 28.0;
const ADD_BUTTON_SIZE: f32 = 18.0;
const TOP_PADDING: f32 = 32.0;
const BOTTOM_PADDING: f32 = 96.0;

#[component]
pub(super) fn document(handle: &MarkdownHandle, paint: Paint) -> impl View {
    let (rows, revision, list) = (handle.rows, handle.revision, handle.list);
    let handle = *handle;
    view! {
        <div test_id="markdown-document" flex_1 min_w=0.0 min_h=0.0 relative>
            <virtual_list count={move || rows.with(|rows| rows.len())} row_height=48.0
                          row_identity={move |index| RowKey::new(rows.with_untracked(|r| r.key(index)), revision.get())}
                          measure_rows={|| u64::from(root_font_size().get().to_bits())} handle=list size_full
                          let:index>
                {row(&handle, index, &paint)}
            </virtual_list>
            <div absolute right=0.0 top=0.0 bottom=0.0>{ListScrollbar::list(list)}</div>
            <show when={move || rows.with(|rows| rows.len() == 0)}>
                <div test_id="markdown-empty" absolute inset=0.0 flex items_center justify_center>
                    <empty_state icon=IconName::FileText title="Nothing to review" detail="This document is empty."/>
                </div>
            </show>
        </div>
    }
}

fn row(handle: &MarkdownHandle, index: usize, paint: &Paint) -> Option<AnyView> {
    let rows = handle.rows.get_untracked();
    let row = rows.get(index)?;
    let (handle, theme) = (*handle, UiTheme::current());
    let target = row.target;
    let active = handle.selected;
    let selected = memo(move || target.is_some() && active.get() == target);
    let hovered = signal(false);
    let block_hovered = signal(false);
    let base = row.background(paint);
    let block = row
        .block(paint)
        .on_hover(move |inside| {
            block_hovered.set_if_changed(inside);
        })
        .bg(move || {
            if selected.get() {
                theme.surface_selected.get()
            } else if target.is_some() && block_hovered.get() {
                theme.surface_hover.get()
            } else {
                base()
            }
        });
    let block = block.when_some(target, |block, target| {
        block.cursor(Cursor::Pointer).on_click(move |_| handle.select(target, MarkdownFocusPane::Document))
    });
    let top = if index == 0 { TOP_PADDING } else { row.gap };
    let bottom = if index + 1 == rows.len() { BOTTOM_PADDING } else { 0.0 };
    let view = view! {
        <div test_id={format!("markdown-row-{index}")} w_full px=24.0 pt=top pb=bottom flex row justify_center>
            <div w_full max_w=COLUMN_WIDTH flex row
                 on_hover={move |inside| {
                     hovered.set_if_changed(inside);
                 }}>
                <gutter handle={&handle} row selected hovered/>
                <div flex_1 min_w=0.0 pl=row.indent() flex col>
                    {block}
                    {target.map(|target| thread(&handle, target))}
                    {target.map(|target| draft(&handle, target))}
                </div>
            </div>
        </div>
    };
    Some(view.into_any())
}

#[component]
fn gutter(handle: &MarkdownHandle, row: &Row, selected: Memo<bool>, hovered: Signal<bool>) -> Div {
    let handle = *handle;
    let add = row.target.map(|target| {
        let offset = (row.first_line_center() - ADD_BUTTON_SIZE / 2.0).max(1.0);
        view! {
            <show when={move || hovered.get()}>
                <add_comment handle target offset/>
            </show>
        }
    });
    view! {
        <div w=GUTTER_WIDTH flex_none relative>
            <show when={move || selected.get()}>
                <div absolute top=0.0 bottom=0.0 right=3.0 w=px(2.0) rounded=px(1.0) bg=UiTheme::current().accent/>
            </show>
            {add}
        </div>
    }
}

#[component]
fn add_comment(handle: MarkdownHandle, target: MarkdownTargetId, offset: f32) -> impl View {
    let palette = handle.theme;
    view! {
        <div test_id={format!("markdown-add-comment-{}", target.index())} absolute left=4.0 top=offset
             size=ADD_BUTTON_SIZE rounded={ADD_BUTTON_SIZE / 2.0}
             bg={move || palette.with(|theme| color(theme.diff.accent))} cursor=Cursor::Pointer flex items_center
             justify_center
             on_click={move |event| {
                 event.stop_propagation();
                 handle.begin_comment_on(target);
             }}>
            {Icon::new(IconName::Plus, move || palette.with(|theme| color(theme.diff.background))).size(12.0)}
        </div>
    }
}

fn thread(handle: &MarkdownHandle, target: MarkdownTargetId) -> impl View + use<> {
    let (handle, annotations) = (*handle, handle.annotations);
    dynamic(move || {
        annotations.get();
        let comments = handle.with(|session| {
            session.review().comments_for_target(session.document(), target).cloned().collect::<Vec<_>>()
        });
        let last = comments.len().checked_sub(1)?;
        Some(view! {
            <div pt=8.0 pb=4.0>
                <div test_id={format!("markdown-comment-thread-{}", target.index())} w_full flex col
                     rounded=RADIUS_CONTROL border=px(1.0) border_color=UiTheme::current().border overflow_hidden>
                    {..comments
                        .into_iter()
                        .enumerate()
                        .map(|(index, comment)| card(&handle, target, comment, index == last))}
                </div>
            </div>
        })
    })
}

fn card(
    handle: &MarkdownHandle,
    target: MarkdownTargetId,
    comment: MarkdownReviewComment,
    last: bool,
) -> impl View + use<> {
    let (handle, theme, id) = (*handle, UiTheme::current(), comment.id);
    let title = if comment.outdated { "Outdated comment" } else { "Your comment" };
    view! {
        <div test_id={format!("markdown-comment-{id}")} w_full flex col when:border_b=(!last, px(1.0))
             when:border_color=(!last, theme.border)>
            <div px=12.0 py=8.0 bg=theme.surface_selected flex row items_center gap=8.0>
                <div flex_1 min_w=0.0><text label muted>{title}</text></div>
                <icon_button id={format!("Edit-comment-{id}")} icon=IconName::Pencil label="Edit comment"
                             keyboard_focusable on_click={move || handle.edit_comment(target, id)}/>
                <icon_button id={format!("Delete-comment-{id}")} icon=IconName::X label="Delete comment"
                             keyboard_focusable on_click={move || handle.delete_comment(id)}/>
            </div>
            <div p=12.0 bg=theme.surface><text body primary>{comment.body}</text></div>
        </div>
    }
}

fn draft(handle: &MarkdownHandle, target: MarkdownTargetId) -> impl View + use<> {
    let (handle, draft) = (*handle, handle.draft);
    dynamic(move || {
        let (_, editing) = draft.get().filter(|(id, _)| *id == target)?;
        let body = handle.with(|session| session.draft().map(|draft| draft.body().to_owned()).unwrap_or_default());
        Some(view! {
            <div pt=8.0 pb=4.0>
                {composer(
                    if editing.is_some() { "Edit comment" } else { "Add a comment" }.to_owned(),
                    body,
                    editing.is_some(),
                    move |body| handle.set_draft_body(body),
                    move |body| {
                        handle.set_draft_body(body.to_owned());
                        handle.run(ReviewCommand::SubmitComment);
                    },
                    move || handle.run(ReviewCommand::Cancel),
                )}
            </div>
        })
    })
}
