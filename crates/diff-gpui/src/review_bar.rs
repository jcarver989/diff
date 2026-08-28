use crate::{Cancel, CopyReview, DiffViewer, SubmitReview, style::color};
use gpui::{Context, Div, div, prelude::*, px};

impl DiffViewer {
    pub(crate) fn render_review_bar(&self, cx: &mut Context<Self>) -> Div {
        let palette = self.theme().palette();
        div()
            .h(px(44.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .border_t_1()
            .border_color(color(palette.border))
            .child(
                div()
                    .flex_1()
                    .text_color(color(palette.muted))
                    .child(format!("{} review comments", self.review().len())),
            )
            .when(!self.review().is_empty(), |bar| {
                bar.child(
                    button("copy-review", "Copy", palette.accent).on_click(cx.listener(
                        |viewer, _, window, cx| {
                            viewer.copy_review(&CopyReview, window, cx);
                        },
                    )),
                )
                .child(
                    button("submit-review", "Submit", palette.accent).on_click(cx.listener(
                        |viewer, _, window, cx| viewer.submit_review(&SubmitReview, window, cx),
                    )),
                )
            })
            .child(
                button("cancel-review", "Cancel", palette.muted).on_click(
                    cx.listener(|viewer, _, window, cx| viewer.cancel(&Cancel, window, cx)),
                ),
            )
    }
}

fn button(
    id: &'static str,
    label: &'static str,
    foreground: diff_core::Rgba,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .px_2()
        .py_1()
        .rounded_sm()
        .cursor_pointer()
        .text_color(color(foreground))
        .hover(|value| value.opacity(0.8))
        .child(label)
}
