use crate::{DiffViewer, style::color};
use diff_core::{FileStatus, StageState};
use gpui::{Context, Div, div, prelude::*, px};

impl DiffViewer {
    pub(crate) fn render_sidebar(&self, cx: &mut Context<Self>) -> Div {
        let palette = &self.theme().palette;
        let mut files = div().id("diff-files").flex_1().overflow_y_scroll().py_2();
        for (index, file) in self.document().files.iter().enumerate() {
            let status_color = match file.status {
                FileStatus::Added | FileStatus::Untracked => palette.addition,
                FileStatus::Deleted => palette.deletion,
                FileStatus::Modified | FileStatus::Renamed | FileStatus::Copied => palette.accent,
            };
            let stage = match file.staged {
                StageState::Staged => "●",
                StageState::PartiallyStaged => "◐",
                StageState::Unstaged => "○",
            };
            let selected = self.selected_file() == Some(index);
            files = files.child(
                div()
                    .id(("diff-file", index))
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .cursor_pointer()
                    .when(selected, |row| row.bg(color(palette.selection)))
                    .hover(|row| row.bg(color(palette.selection)))
                    .on_click(cx.listener(move |viewer, _, _, cx| viewer.select_file(index, cx)))
                    .child(
                        div()
                            .w(px(14.0))
                            .text_color(color(status_color))
                            .child(status_marker(file.status)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(file.path.to_string()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(color(palette.muted))
                            .child(format!(
                                "+{} −{} {stage}",
                                file.additions(),
                                file.deletions()
                            )),
                    ),
            );
        }

        div()
            .w(px(self.options().sidebar_width))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(color(palette.border))
            .child(
                div()
                    .h(px(52.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .px_4()
                    .border_b_1()
                    .border_color(color(palette.border))
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(format!("CHANGED FILES  {}", self.document().files.len())),
            )
            .child(files)
    }
}

const fn status_marker(status: FileStatus) -> &'static str {
    match status {
        FileStatus::Modified => "M",
        FileStatus::Added => "A",
        FileStatus::Deleted => "D",
        FileStatus::Renamed => "R",
        FileStatus::Copied => "C",
        FileStatus::Untracked => "?",
    }
}
