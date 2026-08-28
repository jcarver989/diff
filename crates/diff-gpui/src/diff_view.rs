use crate::{DiffViewer, style};
use diff_core::{DiffSide, PresentedCell, PresentedRow, RowKind};
use gpui::{
    AnyElement, Context, Div, HighlightStyle, SharedString, StyledText, div, prelude::*, px,
    uniform_list,
};
use std::ops::Range;

impl DiffViewer {
    pub(crate) fn render_diff(&mut self, cx: &mut Context<Self>) -> Div {
        let palette = self.theme().palette.clone();
        let Some(file_index) = self.selected_file() else {
            return div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(style::color(palette.muted))
                .child("No changes to review");
        };
        let file = &self.document().files[file_index];
        let Some(file_range) = self.presentation().file_range(file_index) else {
            return div().flex_1();
        };
        let count = file_range.len();
        let start = file_range.start;
        let list = uniform_list(
            ("diff-rows", file_index),
            count,
            cx.processor(move |viewer, range: Range<usize>, _window, cx| {
                viewer.render_visible_rows(start, range, cx)
            }),
        )
        .h_full();

        let header = div()
            .h(px(52.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px_4()
            .border_b_1()
            .border_color(style::color(palette.border))
            .child(
                div()
                    .flex_1()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(file.path.to_string()),
            )
            .child(
                div()
                    .text_color(style::color(palette.addition))
                    .child(format!("+{}", file.additions())),
            )
            .child(
                div()
                    .ml_2()
                    .text_color(style::color(palette.deletion))
                    .child(format!("−{}", file.deletions())),
            );

        div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(header)
            .child(div().flex_1().overflow_hidden().child(list))
    }

    fn render_visible_rows(
        &mut self,
        file_start: usize,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.record_visible_range(range.clone());
        range
            .filter_map(|offset| {
                let index = file_start + offset;
                let row = self.presentation().row(index)?.clone();
                Some(self.render_presented_row(index, &row, cx))
            })
            .collect()
    }

    fn render_presented_row(
        &mut self,
        index: usize,
        row: &PresentedRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.theme().palette.clone();
        if row.kind != RowKind::Code {
            let text = row
                .right
                .as_ref()
                .or(row.left.as_ref())
                .map_or_else(SharedString::default, |cell| cell.text.to_string().into());
            return div()
                .id(("diff-row", row.id.0))
                .h(px(self.options().row_height))
                .w_full()
                .flex()
                .items_center()
                .px_3()
                .bg(style::color(palette.selection))
                .border_b_1()
                .border_color(style::color(palette.border))
                .text_color(style::color(palette.accent))
                .child(text)
                .into_any_element();
        }

        match self.presentation().view_mode() {
            diff_core::ViewMode::Split => div()
                .id(("diff-row", row.id.0))
                .h(px(self.options().row_height))
                .w_full()
                .flex()
                .child(self.render_cell(index, DiffSide::Old, row.left.as_ref(), true, cx))
                .child(self.render_cell(index, DiffSide::New, row.right.as_ref(), false, cx))
                .into_any_element(),
            diff_core::ViewMode::Unified | diff_core::ViewMode::Auto => {
                let (side, cell) = row
                    .right
                    .as_ref()
                    .map(|cell| (DiffSide::New, cell))
                    .or_else(|| row.left.as_ref().map(|cell| (DiffSide::Old, cell)))
                    .expect("code rows contain a cell");
                div()
                    .id(("diff-row", row.id.0))
                    .h(px(self.options().row_height))
                    .w_full()
                    .flex()
                    .child(self.render_cell(index, side, Some(cell), false, cx))
                    .into_any_element()
            }
        }
    }

    fn render_cell(
        &mut self,
        index: usize,
        side: DiffSide,
        cell: Option<&PresentedCell>,
        left: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.theme().palette.clone();
        let Some(cell) = cell else {
            return div()
                .w_1_2()
                .h_full()
                .bg(style::color(palette.gutter))
                .when(left, |value| {
                    value
                        .border_r_1()
                        .border_color(style::color(palette.border))
                })
                .into_any_element();
        };
        let text: SharedString = cell.text.to_string().into();
        let highlights = self.highlight_cell(index, side, &cell.text);
        let styled = StyledText::new(text).with_highlights(highlights.into_iter().map(|span| {
            let style: HighlightStyle = style::highlight_style(span.foreground, span.font_style);
            (span.range, style)
        }));
        let tone = cell.tone;
        let marker = match tone {
            diff_core::DiffTone::Added => "+",
            diff_core::DiffTone::Removed => "−",
            diff_core::DiffTone::Context | diff_core::DiffTone::Meta => " ",
        };
        let target = cell
            .anchor
            .clone()
            .map(|anchor| (anchor, cell.text.to_string()));
        let comments = cell.anchor.as_ref().map_or(0, |anchor| {
            self.review()
                .comments()
                .iter()
                .filter(|comment| &comment.anchor == anchor)
                .count()
        });
        div()
            .id((if left { "old-cell" } else { "new-cell" }, index))
            .when(
                self.presentation().view_mode() == diff_core::ViewMode::Split,
                gpui::Styled::w_1_2,
            )
            .when(
                self.presentation().view_mode() != diff_core::ViewMode::Split,
                gpui::Styled::w_full,
            )
            .h_full()
            .flex()
            .items_center()
            .overflow_hidden()
            .bg(style::color(style::tone_background(&palette, tone)))
            .when(left, |value| {
                value
                    .border_r_1()
                    .border_color(style::color(palette.border))
            })
            .when_some(target, |value, (anchor, line_text)| {
                value
                    .cursor_pointer()
                    .hover(|hover| hover.bg(style::color(palette.selection)))
                    .on_click(cx.listener(move |viewer, _, window, cx| {
                        viewer.begin_comment(anchor.clone(), line_text.clone(), window, cx);
                    }))
            })
            .child(
                div()
                    .w(px(54.0))
                    .flex_shrink_0()
                    .text_color(style::color(palette.muted))
                    .text_right()
                    .pr_2()
                    .child(
                        cell.line_number
                            .map_or_else(String::new, |number| number.to_string()),
                    ),
            )
            .child(
                div()
                    .w(px(20.0))
                    .flex_shrink_0()
                    .text_color(style::color(style::tone_foreground(&palette, tone)))
                    .child(marker),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(styled),
            )
            .when(comments != 0, |value| {
                value.child(
                    div()
                        .px_2()
                        .text_xs()
                        .text_color(style::color(palette.accent))
                        .child(format!("{comments} 💬")),
                )
            })
            .into_any_element()
    }
}
