use super::{
    inline::{InlineColors, inline_text},
    rows::{CodeSource, DocumentRow, RowKind},
};
use crate::{
    style,
    ui::{
        prelude::{CODE_FONT, IconButton, IconName, Typography, TypographyExt, UiTheme, color, rem},
        tokens::{RADIUS_CONTROL, RADIUS_INNER},
    },
};
use clankerdiff_markdown::{MarkdownInline, MarkdownTableAlignment};
use clankerdiff_syntax::{LanguageHint, SyntaxHighlighter};
use clankerdiff_ui_core::ReviewTheme;
use gpui::{AnyElement, ClipboardItem, Div, FontWeight, Rems, StyledText, div, prelude::*, px};

pub(super) const LIST_INDENT: f32 = 22.0;
pub(super) const BLOCK_PADDING_Y: f32 = 4.0;
pub(super) const CODE_HEADER_HEIGHT: f32 = 32.0;
pub(super) const PROSE: TextScale = TextScale::new(15.0, 24.0);
pub(super) const CODE: TextScale = TextScale::new(13.0, 20.0);
const COMPACT_PROSE: TextScale = TextScale::new(13.0, 20.0);
const COMPACT_CODE: TextScale = TextScale::new(12.0, 18.0);

pub(crate) struct MarkdownContent<'a> {
    pub(crate) theme: &'a ReviewTheme,
    pub(crate) ui: &'a UiTheme,
    pub(crate) inline: InlineColors,
    pub(crate) highlighter: &'a mut SyntaxHighlighter,
    pub(crate) copy: bool,
    pub(crate) compact: bool,
}

impl MarkdownContent<'_> {
    pub(crate) fn rows(&mut self, id: &'static str, rows: &[DocumentRow]) -> Vec<AnyElement> {
        rows.iter()
            .enumerate()
            .map(|(index, row)| {
                div()
                    .id((id, index))
                    .pt(rem(self.gap(row.gap)))
                    .pl(self.indent(row))
                    .child(self.block(row))
                    .into_any_element()
            })
            .collect()
    }

    pub(super) fn indent(&self, row: &DocumentRow) -> Rems {
        rem(self.gap(LIST_INDENT) * f32::from(row.depth))
    }

    pub(crate) fn block(&mut self, row: &DocumentRow) -> Div {
        let colors = self.ui.colors;
        let markdown = &self.theme.markdown;
        let scale = self.prose();
        let prose = || {
            div()
                .px(rem(self.gap(8.0)))
                .py(rem(self.gap(BLOCK_PADDING_Y)))
                .rounded(rem(RADIUS_INNER))
                .text_size(rem(scale.size))
                .line_height(rem(scale.line_height))
        };
        let text = |content: &[MarkdownInline]| inline_text("markdown-text", content, self.inline);
        match &row.kind {
            RowKind::Heading { level, content } => {
                let scale = self.heading(*level);
                prose()
                    .text_size(rem(scale.size))
                    .line_height(rem(scale.line_height))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color(markdown.heading))
                    .child(text(content))
            }
            RowKind::Paragraph { content } => prose().child(text(content)),
            RowKind::ListItem { marker, content } => prose()
                .py(rem(1.0))
                .flex()
                .gap_2()
                .child(div().min_w(rem(14.0)).flex_shrink_0().text_color(colors.text_muted).child(marker.clone()))
                .child(div().flex_1().min_w_0().child(text(content))),
            RowKind::Quote { rows } => div().px_2().child(
                div()
                    .pl_2()
                    .border_l_2()
                    .border_color(color(markdown.quote))
                    .text_color(colors.text_secondary)
                    .flex()
                    .flex_col()
                    .children(self.rows("markdown-quote-row", rows)),
            ),
            RowKind::CodeHeader { code } => self.code_header(code),
            RowKind::CodeLine { code, index } => self.code_line(code, *index),
            RowKind::TableRow { cells, alignments, header, first, last } => {
                self.table_row(cells, alignments, *header, *first, *last)
            }
            RowKind::Rule => div().py(rem(self.gap(12.0))).child(div().h(px(1.0)).bg(colors.border)),
            RowKind::Html { content } => {
                prose().typography(Typography::Code).text_color(colors.text_muted).child(text(content))
            }
        }
    }

    fn code_line(&mut self, code: &CodeSource, line: usize) -> Div {
        let markdown = &self.theme.markdown;
        let scale = if self.compact { COMPACT_CODE } else { CODE };
        let last = line + 1 == code.lines.len();
        div()
            .px(rem(self.gap(16.0)))
            .min_h(rem(scale.line_height))
            .font_family(CODE_FONT)
            .text_size(rem(scale.size))
            .line_height(rem(scale.line_height))
            .text_color(color(markdown.code))
            .bg(color(markdown.code_background))
            .border_l_1()
            .border_r_1()
            .border_color(self.ui.colors.border)
            .when(last, |line| line.pb_2().border_b_1().rounded_b(rem(RADIUS_CONTROL)))
            .child(self.code_text(code, line))
    }

    fn table_row(
        &self,
        cells: &[Vec<MarkdownInline>],
        alignments: &[MarkdownTableAlignment],
        header: bool,
        first: bool,
        last: bool,
    ) -> Div {
        let colors = self.ui.colors;
        let scale = if self.compact { COMPACT_CODE } else { TextScale::new(14.0, 20.0) };
        div()
            .flex()
            .text_size(rem(scale.size))
            .line_height(rem(scale.line_height))
            .border_l_1()
            .border_r_1()
            .border_b_1()
            .border_color(colors.border)
            .when(first, |row| row.border_t_1().rounded_t(rem(RADIUS_CONTROL)))
            .when(last, |row| row.rounded_b(rem(RADIUS_CONTROL)))
            .when(header, |row| row.bg(colors.surface).font_weight(FontWeight::SEMIBOLD))
            .children(cells.iter().enumerate().map(|(column, cell)| {
                let alignment = alignments.get(column).copied().unwrap_or(MarkdownTableAlignment::None);
                div()
                    .flex_1()
                    .min_w_0()
                    .px(rem(self.gap(12.0)))
                    .py(rem(self.gap(6.0)))
                    .when(column > 0, |cell| cell.border_l_1().border_color(colors.border))
                    .map(|cell| match alignment {
                        MarkdownTableAlignment::Center => cell.text_center(),
                        MarkdownTableAlignment::Right => cell.text_right(),
                        MarkdownTableAlignment::None | MarkdownTableAlignment::Left => cell,
                    })
                    .child(inline_text(("markdown-cell", column), cell, self.inline))
            }))
    }

    fn code_header(&self, code: &CodeSource) -> Div {
        let colors = self.ui.colors;
        let markdown = &self.theme.markdown;
        let copy = self.copy.then(|| {
            let text = code.text.clone();
            IconButton::new("markdown-copy-code", IconName::Copy, "Copy code", *self.ui)
                .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.to_string())))
        });
        div()
            .h(rem(self.gap(CODE_HEADER_HEIGHT)))
            .pl(rem(self.gap(16.0)))
            .pr_1()
            .flex()
            .items_center()
            .justify_between()
            .bg(color(markdown.code_background))
            .border_t_1()
            .border_l_1()
            .border_r_1()
            .border_color(colors.border)
            .rounded_t(rem(RADIUS_CONTROL))
            .when(code.lines.is_empty(), |header| header.border_b_1().rounded_b(rem(RADIUS_CONTROL)))
            .typography(Typography::Caption)
            .text_color(colors.text_muted)
            .child(code.language.clone().unwrap_or_else(|| "code".to_owned()))
            .children(copy)
    }

    fn code_text(&mut self, code: &CodeSource, index: usize) -> StyledText {
        let highlights = self
            .highlighter
            .with_theme(&self.theme.syntax)
            .highlight_document(code.source_id, LanguageHint::InfoString(&code.info), || &code.text)
            .ok();
        let spans = highlights.as_ref().and_then(|highlights| highlights.line(index)).unwrap_or_default();
        style::code_text(code.lines.get(index).cloned().unwrap_or_default().into(), spans)
    }

    fn gap(&self, value: f32) -> f32 {
        if self.compact { value / 2.0 } else { value }
    }

    const fn prose(&self) -> TextScale {
        if self.compact { COMPACT_PROSE } else { PROSE }
    }

    const fn heading(&self, level: u8) -> TextScale {
        if self.compact {
            return COMPACT_PROSE;
        }
        heading_scale(level)
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TextScale {
    pub(super) size: f32,
    pub(super) line_height: f32,
}

impl TextScale {
    const fn new(size: f32, line_height: f32) -> Self {
        Self { size, line_height }
    }
}

pub(super) const fn heading_scale(level: u8) -> TextScale {
    match level {
        1 => TextScale::new(26.0, 34.0),
        2 => TextScale::new(21.0, 30.0),
        3 => TextScale::new(17.0, 26.0),
        _ => PROSE,
    }
}
