use crate::{
    ReviewHandle,
    ui::{
        components::{Button, ButtonVariant, ControlSize, SurfaceExt},
        fonts::UI_FONT,
        theme::UiTheme,
        tokens::{RADIUS_CONTROL, rem, scaled},
        typography::{Typography, TypographyExt},
    },
};
use rouge_core::{Cursor, FocusHandle, Styled, View, div, dynamic, editable, focus_handle, text};
use rouge_reactive::{Memo, memo, provide_context, signal, use_context};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, PartialEq)]
struct DraftFocus {
    handle: FocusHandle,
    opened: Rc<Cell<bool>>,
}

pub(crate) fn install(handle: &ReviewHandle) {
    let draft = handle.draft_placement();
    provide_context(memo(move || {
        draft.with(|draft| {
            draft.as_ref().map(|_| DraftFocus { handle: focus_handle(), opened: Rc::new(Cell::new(false)) })
        })
    }));
}

pub(crate) fn composer(
    title: String,
    body: String,
    editing: bool,
    theme: UiTheme,
    on_change: impl Fn(String) + 'static,
    on_submit: impl Fn(&str) + 'static,
    on_cancel: impl Fn() + 'static,
) -> impl View {
    let value = signal(body);
    let submit = Rc::new(on_submit);
    rouge_reactive::effect(move || on_change(value.get()));
    let mut input = editable(value).test_id("comment-editor");
    let focus = if let Some(draft) = use_context::<Memo<Option<DraftFocus>>>().and_then(|focus| focus.get_untracked()) {
        if !draft.opened.replace(true) {
            input = input.autofocus();
        }
        draft.handle
    } else {
        input = input.autofocus();
        focus_handle()
    };
    let input = input.track_focus(focus);
    let empty = memo(move || value.with(|body| body.trim().is_empty()));
    div()
        .test_id("comment-composer")
        .flex()
        .col()
        .gap(rem(12.0))
        .p(rem(12.0))
        .surface(theme)
        .bg(theme.paint(|c| c.surface_selected))
        .child(text(title).typography(Typography::BodyStrong).color(theme.paint(|c| c.text)))
        .child(
            div()
                .min_h(rem(96.0))
                .w_full()
                .p(rem(12.0))
                .rounded(scaled(RADIUS_CONTROL))
                .border(1.0)
                .border_color(theme.paint(move |c| if focus.is_focused() { c.accent } else { c.border }))
                .bg(theme.paint(|c| c.canvas))
                .cursor(Cursor::Text)
                .child(
                    input
                        .multiline()
                        .min_rows(3)
                        .max_rows(12)
                        .font(UI_FONT)
                        .text_size(rem(13.0))
                        .line_height(1.5)
                        .color(theme.paint(|c| c.text))
                        .w_full()
                        .submit_on_enter(true)
                        .on_submit({
                            let submit = submit.clone();
                            move |body| submit(body)
                        }),
                ),
        )
        .child(
            div()
                .w_full()
                .flex()
                .row()
                .justify_end()
                .items_center()
                .gap(rem(8.0))
                .child(
                    Button::new("cancel-comment", "Cancel", theme)
                        .variant(ButtonVariant::Ghost)
                        .size(ControlSize::Small)
                        .on_click(on_cancel),
                )
                .child(dynamic(move || {
                    let submit = submit.clone();
                    Button::new("save-comment", if editing { "Save" } else { "Comment" }, theme)
                        .size(ControlSize::Small)
                        .variant(ButtonVariant::Primary)
                        .disabled(empty.get())
                        .on_click(move || value.with_untracked(|body| submit(body)))
                })),
        )
}
