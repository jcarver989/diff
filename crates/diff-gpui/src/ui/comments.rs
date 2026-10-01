//! Shared comment presentation components.

use crate::comment_editor::CommentEditor;
use gpui::{
    AnyElement, App, Context, Entity, IntoElement, MouseButton, RenderOnce, SharedString, Window,
    div, prelude::*, px,
};

use super::{
    components::{Surface, icon_button},
    theme::UiTheme,
};

/// Compact saved-comment count.
#[derive(IntoElement)]
pub(crate) struct CommentCount {
    count: usize,
    font_size: f32,
    theme: UiTheme,
}
impl CommentCount {
    pub(crate) fn new(count: usize, font_size: f32, theme: &UiTheme) -> Self {
        Self {
            count,
            font_size,
            theme: *theme,
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
    header_actions: Option<AnyElement>,
}
impl CommentCard {
    pub(crate) fn new(
        id: u64,
        title: impl Into<SharedString>,
        body: impl Into<SharedString>,
        font_size: f32,
        theme: &UiTheme,
        last: bool,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            body: body.into(),
            font_size,
            theme: *theme,
            last,
            header_actions: None,
        }
    }

    pub(crate) fn header_actions(mut self, actions: impl IntoElement) -> Self {
        self.header_actions = Some(actions.into_any_element());
        self
    }
}
impl RenderOnce for CommentCard {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let colors = self.theme.colors;
        div()
            .id(("review-comment", self.id))
            .debug_selector(move || format!("review-comment-{}", self.id))
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
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(div().min_w_0().whitespace_normal().child(self.title))
                    .children(self.header_actions),
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

pub(crate) fn comment_dismiss_button<T: 'static>(
    id: u64,
    font_size: f32,
    theme: &UiTheme,
    cx: &mut Context<T>,
    handler: impl Fn(&mut T, &mut Window, &mut Context<T>) + 'static,
) -> AnyElement {
    div()
        .id(("comment-dismiss-actions", id))
        .key_context("CommentDismissControl")
        .flex()
        .flex_shrink_0()
        .items_center()
        .on_key_down(|event, window, cx| match event.keystroke.key.as_str() {
            "tab" => {
                if event.keystroke.modifiers.shift {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
                cx.stop_propagation();
            }
            "enter" | "space" => cx.stop_propagation(),
            _ => {}
        })
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            icon_button(("dismiss-comment", id), "×", "Dismiss comment", *theme)
                .font_size(font_size * 1.25)
                .tooltip("Dismiss comment")
                .keyboard_focusable()
                .on_click(cx.listener(move |viewer, _, window, cx| {
                    cx.stop_propagation();
                    handler(viewer, window, cx);
                })),
        )
        .into_any_element()
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
        theme: &UiTheme,
        cancel: impl IntoElement,
        submit: impl IntoElement,
    ) -> Self {
        Self {
            editor,
            title: title.into(),
            theme: *theme,
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
