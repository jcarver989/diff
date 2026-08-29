use crate::{DiffViewer, style::color};
use gpui::{Div, Stateful, div, prelude::*, px};

const NAVIGATION: &[(&str, &str)] = &[
    ("j / k, ↑ / ↓", "move selection"),
    ("h / l, ← / →", "change pane, fold, or select side"),
    ("Tab", "change pane"),
    ("g / G, Home / End", "first / last item"),
    ("PgUp / PgDn", "move a page"),
];
const GIT: &[(&str, &str)] = &[
    ("Space", "stage / unstage file or directory"),
    ("a / A", "stage / unstage all"),
    ("C", "commit staged changes"),
    ("d", "discard selected file"),
];
const REVIEW: &[(&str, &str)] = &[
    ("c / e / x", "add / edit / delete comment"),
    ("u", "undo last comment"),
    ("s / y", "submit / copy review"),
    ("v", "cycle layout"),
    ("Esc / Ctrl-G", "cancel review"),
];
const DRAFT: &[(&str, &str)] = &[
    ("Enter", "save comment"),
    ("Shift-Enter", "insert newline"),
    ("Esc", "cancel comment"),
];

impl DiffViewer {
    pub(crate) fn render_shortcuts(&self) -> Stateful<Div> {
        let palette = self.theme().palette();
        let section = |title: &'static str, rows: &'static [(&'static str, &'static str)]| {
            let mut content = div().flex_1().flex().flex_col().gap_1().child(
                div()
                    .mb_1()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(color(palette.accent))
                    .child(title),
            );
            for (keys, description) in rows {
                content = content.child(
                    div()
                        .flex()
                        .gap_3()
                        .child(
                            div()
                                .w(px(100.0))
                                .flex_shrink_0()
                                .text_color(color(palette.foreground))
                                .child(*keys),
                        )
                        .child(div().text_color(color(palette.muted)).child(*description)),
                );
            }
            content
        };

        div()
            .id("shortcut-help-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(color(palette.background))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("shortcut-help")
                    .w(px(680.0))
                    .max_w_full()
                    .p_5()
                    .rounded_md()
                    .border_1()
                    .border_color(color(palette.border))
                    .bg(color(palette.background))
                    .shadow_lg()
                    .child(
                        div()
                            .mb_4()
                            .flex()
                            .justify_between()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("Review shortcuts")
                            .child(
                                div()
                                    .text_color(color(palette.muted))
                                    .child("? / Esc to close"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_5()
                            .child(section("Navigation", NAVIGATION))
                            .child(section("Review", REVIEW)),
                    )
                    .child(
                        div()
                            .mt_4()
                            .flex()
                            .gap_5()
                            .child(section("Git", GIT))
                            .child(section("Comment editor", DRAFT)),
                    ),
            )
    }
}
