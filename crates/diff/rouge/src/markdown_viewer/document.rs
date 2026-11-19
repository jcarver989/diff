use super::MarkdownHandle;
use crate::comment_editor::composer;
use clankerdiff_core::ReviewCommand;
use clankerdiff_markdown::{MarkdownFocusPane, MarkdownReviewComment, MarkdownTargetId};
use rouge_core::{
    AnyView, Apply, Color, Cursor, Div, Rect, Role, RowKey, Styled, View, component, div, dynamic, px, show, text,
    view, virtual_list,
};
use rouge_reactive::{Memo, Signal, memo, signal};
use rouge_widgets::tokens::{GRID_MARGIN, HAIRLINE, ICON_SIZE_SMALL, Radius, Space, UNIT};
use rouge_widgets::{
    IconName, MarkdownPaint, MarkdownRow, UiStyles, column_grid, empty_state, icon_button, reading_cell, reading_span,
    vertical_scrollbar,
};

const READING_WIDTH: f32 = 188.0 * UNIT;
const GUTTER_WIDTH: f32 = 28.0;
const ADD_BUTTON_SIZE: f32 = 20.0;
const TOP_PADDING: f32 = Space::X8.px();
const BOTTOM_PADDING: f32 = Space::X24.px();

#[component]
pub(super) fn document(handle: &MarkdownHandle, paint: MarkdownPaint) -> impl View {
    let (rows, revision, list) = (handle.rows, handle.revision, handle.list);
    let handle = *handle;
    let bounds = signal(Rect::ZERO);
    let reading = memo(move || reading_span(bounds.get().size.width, READING_WIDTH));
    view! {
        <div test_id="markdown-document" flex_1 min_w=0.0 min_h=0.0 relative track_bounds=bounds>
            <virtual_list count={move || rows.with(|rows| rows.len())} row_height=48.0
                          row_identity={move |index| RowKey::new(rows.with_untracked(|r| r.key(index)), revision.get())}
                          measure_rows={move || revision.get()} handle=list size_full
                          let:index>
                {row(&handle, index, &paint, reading)}
            </virtual_list>
            <div absolute right=0.0 top=0.0 bottom=0.0>{vertical_scrollbar(list.scroll_state())}</div>
            <show when={move || rows.with(|rows| rows.len() == 0)}>
                <div test_id="markdown-empty" absolute inset=0.0 flex items_center justify_center>
                    <empty_state icon=IconName::FileText title="Nothing to review" detail="This document is empty."/>
                </div>
            </show>
        </div>
    }
}

fn row(handle: &MarkdownHandle, index: usize, paint: &MarkdownPaint, reading: Memo<u16>) -> Option<AnyView> {
    let rows = handle.rows.get_untracked();
    let row = rows.get(index)?;
    let handle = *handle;
    let target = row.target;
    let active = handle.selected;
    let selected = memo(move || target.is_some() && active.get() == target);
    let hovered = signal(false);
    let block_hovered = signal(false);
    let block = row
        .block(paint)
        .on_hover(move |inside| {
            block_hovered.set_if_changed(inside);
        })
        .when(move || target.is_some() && block_hovered.get(), |style| style.bg(Role::SurfaceHover))
        .when(selected, |style| style.bg(Role::SurfaceSelected));
    let block = block.when_some(target, |block, target| {
        block.cursor(Cursor::Pointer).on_click(move |_| handle.select(target, MarkdownFocusPane::Document))
    });
    let top = if index == 0 { TOP_PADDING } else { row.gap };
    let bottom = if index + 1 == rows.len() { BOTTOM_PADDING } else { 0.0 };
    let view = view! {
        <div test_id={format!("markdown-row-{index}")} w_full pt=top pb=bottom>
            <column_grid>
                <reading_cell test_id={format!("markdown-column-{index}")} span=reading relative flex row
                              on_hover={move |inside| {
                                  hovered.set_if_changed(inside);
                              }}>
                    <gutter handle={&handle} row selected hovered/>
                    <div flex_1 min_w=0.0 pl=row.indent() flex col>
                        {block}
                        {target.map(|target| thread(&handle, target))}
                        {target.map(|target| draft(&handle, target))}
                    </div>
                </reading_cell>
            </column_grid>
        </div>
    };
    Some(view.into_any())
}

#[component]
fn gutter(handle: &MarkdownHandle, row: &MarkdownRow, selected: Memo<bool>, hovered: Signal<bool>) -> Div {
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
        <div absolute top=0.0 bottom=0.0 left={GRID_MARGIN.px() - GUTTER_WIDTH} w=GUTTER_WIDTH>
            <show when={move || selected.get()}>
                <div absolute top=0.0 bottom=0.0 right=Space::X1 w=px(2.0) rounded=Radius::Full bg=Role::Accent/>
            </show>
            {add}
        </div>
    }
}

#[component]
fn add_comment(handle: MarkdownHandle, target: MarkdownTargetId, offset: f32) -> impl View {
    let palette = handle.theme;
    let accent = move || palette.with(|theme| Color::from(theme.diff.accent));
    let button = icon_button(IconName::Plus, "Add comment".to_owned())
        .test_id(format!("markdown-add-comment-{}", target.index()))
        .size(ADD_BUTTON_SIZE)
        .icon_size(ICON_SIZE_SMALL)
        .rounded(Radius::Inner)
        .bg(accent)
        .hover(move |style| style.bg(accent))
        .color(move || palette.with(|theme| Color::from(theme.diff.background)))
        .on_click(move || handle.begin_comment_on(target));
    view! { <div absolute left=Space::X1 top=offset>{button}</div> }
}

fn thread(handle: &MarkdownHandle, target: MarkdownTargetId) -> impl View + use<> {
    let (handle, annotations) = (*handle, handle.annotations);
    let comments = memo(move || {
        annotations.get();
        handle.with(|session| {
            session.review().comments_for_target(session.document(), target).cloned().collect::<Vec<_>>()
        })
    });
    dynamic(move || {
        let comments = comments.get();
        let last = comments.len().checked_sub(1)?;
        Some(view! {
            <div pt=Space::X2 pb=Space::X1>
                <div test_id={format!("markdown-comment-thread-{}", target.index())} w_full flex col
                     rounded=Radius::Control border=HAIRLINE border_color=Role::Border overflow_hidden>
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
    let (handle, id) = (*handle, comment.id);
    let title = if comment.outdated { "Outdated comment" } else { "Your comment" };
    let editing = memo(move || handle.draft.get() == Some((target, Some(id))));
    view! {
        <div test_id={format!("markdown-comment-{id}")} w_full flex col when:border_b=(!last, HAIRLINE)
             when:border_color=(!last, Role::Border)>
            <div px=Space::X3 py=Space::X2 bg=Role::SurfaceSelected flex row items_center gap=Space::X2>
                <div flex_1 min_w=0.0><text label muted>{title}</text></div>
                <show when={move || !editing.get()}>
                    <icon_button test_id={format!("Edit-comment-{id}")} icon=IconName::Pencil label="Edit comment"
                                 on_click={move || handle.edit_comment(target, id)}/>
                    <icon_button test_id={format!("Delete-comment-{id}")} icon=IconName::X label="Delete comment"
                                 on_click={move || handle.delete_comment(id)}/>
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

fn draft(handle: &MarkdownHandle, target: MarkdownTargetId) -> impl View + use<> {
    let (handle, draft) = (*handle, handle.draft);
    view! {
        <show when={move || draft.get() == Some((target, None))}>
            <div pt=Space::X2 pb=Space::X1>
                {editor(handle, Some("Add a comment".to_owned()))}
            </div>
        </show>
    }
}

fn editor(handle: MarkdownHandle, title: Option<String>) -> Div {
    let body = handle.with(|session| session.draft().map_or_default(|draft| draft.body().to_owned()));
    composer(
        title,
        body,
        move |body| handle.set_draft_body(body),
        move |body| {
            handle.set_draft_body(body.to_owned());
            handle.run(ReviewCommand::SubmitComment);
        },
        move || handle.run(ReviewCommand::Cancel),
    )
}
