use crate::{comment_editor::CommentEditor, style};
use diff_core::{DiffPalette, Rgba};
use gpui::{Div, Entity, SharedString, div, prelude::*, px};

/// Builds the marker shown at the end of a target with saved comments.
pub(crate) fn comment_count_marker(count: usize, metadata_font_size: f32, accent: Rgba) -> Div {
    div()
        .flex_shrink_0()
        .px_2()
        .text_size(px(metadata_font_size))
        .text_color(style::color(accent))
        .child(format!("{count} 💬"))
}

/// Builds a hover-revealed add-comment affordance.
///
/// The owning viewer attaches the click handler because only it knows how to
/// select its target and open an editor.
pub(crate) fn add_comment_button(
    id: impl Into<String>,
    hover_group: SharedString,
    palette: &DiffPalette,
) -> gpui::Stateful<Div> {
    div()
        .id(id.into())
        .aria_label("Add comment")
        .absolute()
        .left(px(5.0))
        .top(px(1.0))
        .size(px(18.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .invisible()
        .group_hover(hover_group, gpui::Styled::visible)
        .cursor_pointer()
        .bg(style::color(palette.accent))
        .text_color(style::color(palette.background))
        .font_weight(gpui::FontWeight::BOLD)
        .hover(|button| button.opacity(0.85))
        .child("+")
}

/// Builds one saved-comment card.
pub(crate) fn comment_card(
    id: u64,
    title: impl Into<SharedString>,
    body: impl Into<SharedString>,
    metadata_font_size: f32,
    palette: &DiffPalette,
    last: bool,
) -> gpui::Stateful<Div> {
    div()
        .id(("review-comment", id))
        .w_full()
        .flex()
        .flex_col()
        .when(!last, |comment| {
            comment
                .border_b_1()
                .border_color(style::color(palette.border))
        })
        .child(
            div()
                .px_3()
                .py_2()
                .bg(style::color(palette.selection))
                .text_size(px(metadata_font_size))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(style::color(palette.muted))
                .child(title.into()),
        )
        .child(
            div()
                .p_3()
                .bg(style::color(palette.background))
                .text_color(style::color(palette.foreground))
                .whitespace_normal()
                .child(body.into()),
        )
}

pub(crate) fn comment_editor_card(
    editor: Entity<CommentEditor>,
    title: impl Into<SharedString>,
    can_submit: bool,
    palette: &DiffPalette,
    cancel_button: gpui::Stateful<Div>,
    submit_button: gpui::Stateful<Div>,
) -> Div {
    div()
        .w_full()
        .p_3()
        .flex()
        .flex_col()
        .gap_3()
        .rounded_md()
        .border_1()
        .border_color(style::color(palette.border))
        .bg(style::color(palette.selection))
        .child(
            div()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(style::color(palette.foreground))
                .child(title.into()),
        )
        .child(editor)
        .child(
            div()
                .w_full()
                .flex()
                .justify_end()
                .items_center()
                .gap_2()
                .child(cancel_button)
                .child(submit_button.when(!can_submit, |button| button.opacity(0.45))),
        )
}

pub(crate) fn comment_action_button(
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    foreground: Rgba,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_sm()
        .cursor_pointer()
        .text_color(style::color(foreground))
        .hover(|button| button.opacity(0.8))
        .child(label)
}
