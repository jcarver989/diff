use crate::{DiffViewer, style};
use diff_core::{DiffSide, PresentedCell, PresentedRow, RowKind};
use gpui::{
    AnyElement, Context, Div, HighlightStyle, SharedString, StyledText, div, list, prelude::*, px,
};

const GUTTER_WIDTH: f32 = 54.0;
const MARKER_WIDTH: f32 = 20.0;
const HEADER_HEIGHT: f32 = 52.0;

impl DiffViewer {
    pub(crate) fn render_diff(&mut self, cx: &mut Context<Self>) -> Div {
        let palette = self.theme().palette().clone();
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
        let file_path = file.path.to_string();
        let additions = file.additions();
        let deletions = file.deletions();
        let Some(file_range) = self.presentation().file_range(file_index) else {
            return div().flex_1();
        };
        let start = file_range.start;
        let row_count = file_range.len();
        let split = self.layout().is_split();
        let list_state = self.sync_diff_list(file_index, row_count, split);
        let list = list(
            list_state,
            cx.processor(move |viewer, offset: usize, _window, cx| {
                viewer.render_visible_row(start + offset, cx)
            }),
        )
        .h_full();

        let header = div()
            .h(px(HEADER_HEIGHT))
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
                    .child(file_path),
            )
            .child(
                div()
                    .text_color(style::color(palette.addition))
                    .child(format!("+{additions}")),
            )
            .child(
                div()
                    .ml_2()
                    .text_color(style::color(palette.deletion))
                    .child(format!("−{deletions}")),
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

    fn render_visible_row(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.presentation().row(index).cloned() else {
            return div().into_any_element();
        };
        self.render_presented_row(index, &row, cx)
    }

    fn render_presented_row(
        &mut self,
        index: usize,
        row: &PresentedRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.theme().palette().clone();
        let row_height = px(self.options().row_height);
        if row.kind != RowKind::Code {
            let text = row
                .primary_cell()
                .map_or_else(SharedString::default, |cell| cell.text.to_string().into());
            return div()
                .id(("diff-row", row.id.0))
                .h(row_height)
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

        let split = self.layout().is_split();
        let mut element = div()
            .id(("diff-row", row.id.0))
            .min_h(row_height)
            .w_full()
            .flex();
        if split {
            element = element
                .child(self.render_cell(index, row, DiffSide::Old, row.left.as_ref(), true, cx))
                .child(self.render_cell(index, row, DiffSide::New, row.right.as_ref(), false, cx));
        } else {
            let side = if row.right.is_some() {
                DiffSide::New
            } else {
                DiffSide::Old
            };
            element =
                element.child(self.render_cell(index, row, side, row.primary_cell(), false, cx));
        }
        element.into_any_element()
    }

    fn render_cell(
        &mut self,
        index: usize,
        row: &PresentedRow,
        side: DiffSide,
        cell: Option<&PresentedCell>,
        left: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.theme().palette().clone();
        let split = self.layout().is_split();
        let border_color = style::color(palette.border);
        let Some(cell) = cell else {
            return div()
                .w_1_2()
                .min_h(px(self.options().row_height))
                .bg(style::color(palette.background))
                .when(left, |value| value.border_r_1().border_color(border_color))
                .into_any_element();
        };

        let text: SharedString = cell.text.to_string().into();
        let highlights = self.highlight_cell(row, cell);
        let styled = StyledText::new(text).with_highlights(highlights.into_iter().map(|span| {
            let style: HighlightStyle = style::highlight_style(span.foreground, span.font_style);
            (span.range, style)
        }));
        let tone = cell.tone;
        let colors = palette.tone(tone);
        let commentable = cell.source.is_some();
        let comments = self.comments_on(index, cell);

        let mut element = div()
            .id((if left { "old-cell" } else { "new-cell" }, index))
            .when(split, gpui::Styled::w_1_2)
            .when(!split, gpui::Styled::w_full)
            .min_h(px(self.options().row_height))
            .flex()
            .items_start()
            .py_2()
            .overflow_hidden()
            .bg(style::color(colors.background))
            .when(left, |value| value.border_r_1().border_color(border_color));
        if commentable {
            element = element
                .cursor_pointer()
                .hover(|hover| hover.bg(style::color(palette.selection)))
                .on_click(cx.listener(move |viewer, _, _, cx| {
                    viewer.begin_comment(index, side, cx);
                }));
        }
        element
            .child(
                div()
                    .w(px(GUTTER_WIDTH))
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
                    .w(px(MARKER_WIDTH))
                    .flex_shrink_0()
                    .text_color(style::color(colors.foreground))
                    .child(tone.marker().to_string()),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_normal()
                    .child(styled),
            )
            .when(comments != 0, |value| {
                value.child(
                    div()
                        .flex_shrink_0()
                        .px_2()
                        .text_xs()
                        .text_color(style::color(palette.accent))
                        .child(format!("{comments} 💬")),
                )
            })
            .into_any_element()
    }

    fn comments_on(&self, index: usize, cell: &PresentedCell) -> usize {
        let Some(row) = self.presentation().row(index) else {
            return 0;
        };
        let Some(anchor) = self.presentation().cell_anchor(row, cell) else {
            return 0;
        };
        self.review().comments_for_anchor(&anchor).count()
    }
}
