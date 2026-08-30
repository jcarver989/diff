//! Shared comment presentation components.

use crate::comment_editor::CommentEditor;
use gpui::{
    AnyElement, App, Entity, IntoElement, RenderOnce, SharedString, Window, div, prelude::*, px,
};

use super::{components::Surface, theme::UiTheme};

/// Compact saved-comment count.
#[derive(IntoElement)]
pub(crate) struct CommentCount {
    count: usize,
    font_size: f32,
    theme: UiTheme,
}
impl CommentCount {
    pub(crate) fn new(count: usize, font_size: f32, theme: UiTheme) -> Self {
        Self {
            count,
            font_size,
            theme,
        }
    }
}
impl RenderOnce for CommentCount {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .flex_shrink_0()
            .px_2()
            .text_size(px(self.font_size))
            .text_color(self.theme.colors.accent)
            .child(format!("{} 💬", self.count))
    }
}

/// One saved review comment.
#[derive(IntoElement)]
pub(crate) struct CommentCard {
    id: u64,
    title: SharedString,
    body: SharedString,
    font_size: f32,
    theme: UiTheme,
    last: bool,
}
impl CommentCard {
    pub(crate) fn new(
        id: u64,
        title: impl Into<SharedString>,
        body: impl Into<SharedString>,
        font_size: f32,
        theme: UiTheme,
        last: bool,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            body: body.into(),
            font_size,
            theme,
            last,
        }
    }
}
impl RenderOnce for CommentCard {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let colors = self.theme.colors;
        div()
            .id(("review-comment", self.id))
            .w_full()
            .flex()
            .flex_col()
            .when(!self.last, |comment| {
                comment.border_b_1().border_color(colors.border)
            })
            .child(
                div()
                    .px_3()
                    .py_2()
                    .bg(colors.surface_selected)
                    .text_size(px(self.font_size))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(colors.text_muted)
                    .child(self.title),
            )
            .child(
                div()
                    .p_3()
                    .bg(colors.surface)
                    .text_color(colors.text)
                    .whitespace_normal()
                    .child(self.body),
            )
    }
}

/// A titled comment editor with standardized action placement.
#[derive(IntoElement)]
pub(crate) struct CommentComposer {
    editor: Entity<CommentEditor>,
    title: SharedString,
    theme: UiTheme,
    cancel: AnyElement,
    submit: AnyElement,
}
impl CommentComposer {
    pub(crate) fn new(
        editor: Entity<CommentEditor>,
        title: impl Into<SharedString>,
        theme: UiTheme,
        cancel: impl IntoElement,
        submit: impl IntoElement,
    ) -> Self {
        Self {
            editor,
            title: title.into(),
            theme,
            cancel: cancel.into_any_element(),
            submit: submit.into_any_element(),
        }
    }
}
impl RenderOnce for CommentComposer {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let colors = self.theme.colors;
        Surface::new(self.theme).selected(true).child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(colors.text)
                        .child(self.title),
                )
                .child(self.editor)
                .child(
                    div()
                        .w_full()
                        .flex()
                        .justify_end()
                        .items_center()
                        .gap_2()
                        .child(self.cancel)
                        .child(self.submit),
                ),
        )
    }
}
