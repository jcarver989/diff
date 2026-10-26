//! Shared comment presentation components.

use crate::comment_editor::CommentEditor;
use clankerdiff_gpui_ui::prelude::{
    Button, ButtonVariant, ControlSize, IconButton, IconName, Surface, Typography, TypographyExt, UiTheme,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, IntoElement, MouseButton, RenderOnce, SharedString,
    Window, div, prelude::*,
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Compact saved-comment count.
#[derive(IntoElement)]
pub(crate) struct CommentCount {
    count: usize,
    theme: UiTheme,
}
impl CommentCount {
    pub(crate) fn new(count: usize, theme: &UiTheme) -> Self {
        Self { count, theme: *theme }
    }
}
impl RenderOnce for CommentCount {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .flex_shrink_0()
            .px_2()
            .typography(Typography::Caption)
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
    theme: UiTheme,
    last: bool,
    header_actions: Option<AnyElement>,
}
impl CommentCard {
    pub(crate) fn new(id: u64, title: impl Into<SharedString>, body: impl Into<SharedString>, theme: &UiTheme) -> Self {
        Self { id, title: title.into(), body: body.into(), theme: *theme, last: false, header_actions: None }
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
            .when(!self.last, |comment| comment.border_b_1().border_color(colors.border))
            .child(
                div()
                    .px_3()
                    .py_2()
                    .bg(colors.surface_selected)
                    .typography(Typography::Label)
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
                    .typography(Typography::Body)
                    .text_color(colors.text)
                    .whitespace_normal()
                    .child(self.body),
            )
    }
}

#[derive(IntoElement)]
pub(crate) struct CommentThread {
    id: ElementId,
    theme: UiTheme,
    cards: Vec<CommentCard>,
}
impl CommentThread {
    pub(crate) fn new(id: impl Into<ElementId>, theme: &UiTheme) -> Self {
        Self { id: id.into(), theme: *theme, cards: Vec::new() }
    }

    pub(crate) fn cards(mut self, cards: impl IntoIterator<Item = CommentCard>) -> Self {
        self.cards.extend(cards);
        self
    }
}
impl RenderOnce for CommentThread {
    fn render(mut self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        if let Some(card) = self.cards.last_mut() {
            card.last = true;
        }
        let selector = self.id.to_string();
        div()
            .id(self.id)
            .debug_selector(move || selector)
            .w_full()
            .flex()
            .flex_col()
            .rounded_md()
            .border_1()
            .border_color(self.theme.colors.border)
            .overflow_hidden()
            .children(self.cards)
    }
}

pub(crate) fn comment_dismiss_button<T: 'static>(
    id: u64,
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
            IconButton::new(("dismiss-comment", id), IconName::X, "Dismiss comment", *theme)
                .keyboard_focusable()
                .on_click(cx.listener(move |viewer, _, window, cx| {
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
    on_cancel: Option<ClickHandler>,
    on_submit: Option<ClickHandler>,
}
impl CommentComposer {
    pub(crate) fn new(editor: Entity<CommentEditor>, title: impl Into<SharedString>, theme: &UiTheme) -> Self {
        Self { editor, title: title.into(), theme: *theme, on_cancel: None, on_submit: None }
    }

    pub(crate) fn on_cancel(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_cancel = Some(Box::new(handler));
        self
    }

    pub(crate) fn on_submit(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_submit = Some(Box::new(handler));
        self
    }
}
impl RenderOnce for CommentComposer {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = self.theme.colors;
        let can_submit = !self.editor.read(cx).is_blank();
        let cancel = Button::new("cancel-comment", "Cancel", self.theme)
            .size(ControlSize::Small)
            .when_some(self.on_cancel, |button, handler| button.on_click(handler));
        let submit = Button::new("submit-comment", "Comment", self.theme)
            .variant(ButtonVariant::Primary)
            .size(ControlSize::Small)
            .disabled(!can_submit)
            .when_some(self.on_submit, |button, handler| button.on_click(handler));
        Surface::new(self.theme).selected(true).child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().typography(Typography::BodyStrong).text_color(colors.text).child(self.title))
                .child(self.editor)
                .child(div().w_full().flex().justify_end().items_center().gap_2().child(cancel).child(submit)),
        )
    }
}
