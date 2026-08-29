use crate::{
    Cancel, CopyReview, DiffViewer, ShowThemePicker, SubmitReview, ViewerPane, style::color,
};
use gpui::{Context, Div, div, prelude::*, px};

impl DiffViewer {
    pub(crate) fn render_review_bar(&self, cx: &mut Context<Self>) -> Div {
        let palette = self.theme().palette();
        let hint = if self.comment_editor.is_some() {
            "Enter save · Shift-Enter newline · Esc cancel"
        } else if self.pane == ViewerPane::Files {
            "j/k entry · h/l fold/open · Tab pane · ? help"
        } else if self.layout().is_split() {
            "j/k line · ←/→ side · c comment · s submit · ? help"
        } else {
            "j/k line · c comment · e/x edit/delete · s submit · y copy · ? help"
        };
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
                    .min_w_0()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(color(palette.muted))
                    .child(format!(
                        "{hint}    ·    {} review comments",
                        self.review().len()
                    )),
            )
            .child(
                button("select-theme", "Theme", palette.muted).on_click(cx.listener(
                    |viewer, _, window, cx| {
                        viewer.show_theme_picker(&ShowThemePicker, window, cx);
                    },
                )),
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
