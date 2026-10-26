use super::{
    MarkdownReviewer,
    inline::inline_text,
    rows::{CodeSource, DocumentRow, RowKind},
};
use crate::{
    annotation::add_comment_button,
    comments::{CommentCard, CommentComposer, CommentThread, comment_dismiss_button},
    style,
    ui::prelude::{CODE_FONT, EmptyState, IconButton, IconName, NoticeTone, Typography, TypographyExt, color, rem},
};
use clankerdiff_core::ReviewCommand;
use clankerdiff_markdown::{MarkdownFocusPane, MarkdownInline, MarkdownTableAlignment, MarkdownTargetId};
use clankerdiff_syntax::LanguageHint;
use gpui::{AnyElement, ClipboardItem, Context, Div, FontWeight, Rems, SharedString, StyledText, div, prelude::*, px};

const COLUMN_WIDTH: f32 = 780.0;
const GUTTER_WIDTH: f32 = 28.0;
const LIST_INDENT: f32 = 22.0;
const ADD_BUTTON_SIZE: f32 = 18.0;
const BLOCK_PADDING_Y: f32 = 4.0;
const CODE_HEADER_HEIGHT: f32 = 32.0;
const CODE_RADIUS: f32 = 8.0;
const PROSE: TextScale = TextScale::new(15.0, 24.0);
const CODE: TextScale = TextScale::new(13.0, 20.0);

impl MarkdownReviewer {
    pub(super) fn render_document(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("markdown-document")
            .debug_selector(|| "markdown-document".to_owned())
            .track_scroll(&self.document_scroll)
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .pt_8()
            .pb(rem(96.0))
            .when(self.rows.is_empty(), |document| {
                document.child(div().debug_selector(|| "markdown-empty".to_owned()).size_full().child(EmptyState::new(
                    "Nothing to review",
                    "This document is empty.",
                    NoticeTone::Neutral,
                    self.ui,
                )))
            })
            .children(self.rows.iter().enumerate().map(|(index, row)| self.render_row(index, row, cx)))
    }

    fn render_row(&self, index: usize, row: &DocumentRow, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.ui.colors;
        let selected = row.target.is_some() && row.target == self.session.selected_target();
        let group: SharedString = format!("markdown-row-{index}").into();
        let block = self
            .render_block(row)
            .id("markdown-block")
            .when(selected, |block| block.bg(colors.surface_selected))
            .when_some(row.target, |block, target| {
                block
                    .cursor_pointer()
                    .when(!selected, |block| block.hover(move |block| block.bg(colors.surface_hover)))
                    .on_click(cx.listener(move |reviewer, _, window, cx| {
                        reviewer.select(target, MarkdownFocusPane::Document, window, cx);
                    }))
            });

        div()
            .id(("markdown-row", index))
            .w_full()
            .px_6()
            .pt(rem(row.gap))
            .flex()
            .justify_center()
            .child(
                div()
                    .when_some(row.target, |column, target| {
                        column.debug_selector(move || format!("markdown-row-{}", target.index()))
                    })
                    .group(group.clone())
                    .w_full()
                    .max_w(rem(COLUMN_WIDTH))
                    .flex()
                    .child(self.render_gutter(row, selected, group, cx))
                    .child(div().flex_1().min_w_0().pl(indent(row)).flex().flex_col().child(block).when_some(
                        row.target,
                        |column, target| {
                            column.children(self.render_thread(target, cx)).children(self.render_composer(target, cx))
                        },
                    )),
            )
            .into_any_element()
    }

    fn render_gutter(&self, row: &DocumentRow, selected: bool, group: SharedString, cx: &mut Context<Self>) -> Div {
        let offset = (first_line_center(&row.kind) - ADD_BUTTON_SIZE / 2.0 - 1.0).max(0.0);
        div()
            .w(rem(GUTTER_WIDTH))
            .flex_shrink_0()
            .relative()
            .when(selected, |gutter| {
                gutter.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right(rem(3.0))
                        .w(px(2.0))
                        .rounded_full()
                        .bg(self.ui.colors.accent),
                )
            })
            .when_some(row.target, |gutter, target| {
                gutter.child(
                    div().relative().mt(rem(offset)).child(
                        add_comment_button(format!("markdown-add-comment-{}", target.index()), group, &self.theme.diff)
                            .debug_selector(move || format!("markdown-add-comment-{}", target.index()))
                            .on_click(cx.listener(move |reviewer, _, window, cx| {
                                cx.stop_propagation();
                                reviewer.select(target, MarkdownFocusPane::Document, window, cx);
                                reviewer.run(ReviewCommand::BeginComment, window, cx);
                            })),
                    ),
                )
            })
    }

    fn render_block(&self, row: &DocumentRow) -> Div {
        let colors = self.ui.colors;
        let markdown = &self.theme.markdown;
        let prose = || {
            div()
                .px_2()
                .py(rem(BLOCK_PADDING_Y))
                .rounded_md()
                .text_size(rem(PROSE.size))
                .line_height(rem(PROSE.line_height))
        };
        let text = |content: &[MarkdownInline]| inline_text("markdown-text", content, self.inline);
        match &row.kind {
            RowKind::Heading { level, content } => {
                let scale = heading_scale(*level);
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
                    .children(rows.iter().enumerate().map(|(index, row)| {
                        div()
                            .id(("markdown-quote-row", index))
                            .pt(rem(row.gap))
                            .pl(indent(row))
                            .child(self.render_block(row))
                    })),
            ),
            RowKind::CodeHeader { code } => self.render_code_header(code),
            RowKind::CodeLine { code, index } => self.render_code_line(code, *index),
            RowKind::TableRow { cells, alignments, header, first, last } => {
                self.render_table_row(cells, alignments, *header, *first, *last)
            }
            RowKind::Rule => div().py_3().child(div().h(px(1.0)).bg(colors.border)),
            RowKind::Html { content } => {
                prose().typography(Typography::Code).text_color(colors.text_muted).child(text(content))
            }
        }
    }

    fn render_code_line(&self, code: &CodeSource, line: usize) -> Div {
        let markdown = &self.theme.markdown;
        let last = line + 1 == code.lines.len();
        div()
            .px_4()
            .min_h(rem(CODE.line_height))
            .font_family(CODE_FONT)
            .text_size(rem(CODE.size))
            .line_height(rem(CODE.line_height))
            .text_color(color(markdown.code))
            .bg(color(markdown.code_background))
            .border_l_1()
            .border_r_1()
            .border_color(self.ui.colors.border)
            .when(last, |line| line.pb_2().border_b_1().rounded_b(rem(CODE_RADIUS)))
            .child(self.code_line_text(code, line))
    }

    fn render_table_row(
        &self,
        cells: &[Vec<MarkdownInline>],
        alignments: &[MarkdownTableAlignment],
        header: bool,
        first: bool,
        last: bool,
    ) -> Div {
        let colors = self.ui.colors;
        div()
            .flex()
            .text_size(rem(14.0))
            .line_height(rem(20.0))
            .border_l_1()
            .border_r_1()
            .border_b_1()
            .border_color(colors.border)
            .when(first, |row| row.border_t_1().rounded_t(rem(6.0)))
            .when(last, |row| row.rounded_b(rem(6.0)))
            .when(header, |row| row.bg(colors.surface).font_weight(FontWeight::SEMIBOLD))
            .children(cells.iter().enumerate().map(|(column, cell)| {
                let alignment = alignments.get(column).copied().unwrap_or(MarkdownTableAlignment::None);
                div()
                    .flex_1()
                    .min_w_0()
                    .px_3()
                    .py_1p5()
                    .when(column > 0, |cell| cell.border_l_1().border_color(colors.border))
                    .map(|cell| match alignment {
                        MarkdownTableAlignment::Center => cell.text_center(),
                        MarkdownTableAlignment::Right => cell.text_right(),
                        MarkdownTableAlignment::None | MarkdownTableAlignment::Left => cell,
                    })
                    .child(inline_text(("markdown-cell", column), cell, self.inline))
            }))
    }

    fn render_code_header(&self, code: &CodeSource) -> Div {
        let colors = self.ui.colors;
        let markdown = &self.theme.markdown;
        let copy = self.capabilities.clipboard.then(|| {
            let text = code.text.clone();
            IconButton::new("markdown-copy-code", IconName::Copy, "Copy code", self.ui)
                .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.to_string())))
        });
        div()
            .h(rem(CODE_HEADER_HEIGHT))
            .pl_4()
            .pr_1()
            .flex()
            .items_center()
            .justify_between()
            .bg(color(markdown.code_background))
            .border_t_1()
            .border_l_1()
            .border_r_1()
            .border_color(colors.border)
            .rounded_t(rem(CODE_RADIUS))
            .when(code.lines.is_empty(), |header| header.border_b_1().rounded_b(rem(CODE_RADIUS)))
            .typography(Typography::Caption)
            .text_color(colors.text_muted)
            .child(code.language.clone().unwrap_or_else(|| "code".to_owned()))
            .children(copy)
    }

    fn render_thread(&self, target: MarkdownTargetId, cx: &mut Context<Self>) -> Option<Div> {
        let theme = self.ui;
        let cards = self
            .review()
            .comments_for_target(self.document(), target)
            .map(|comment| {
                let id = comment.id;
                let title = if comment.outdated { "Outdated comment" } else { "Your comment" };
                CommentCard::new(id, title, comment.body.clone(), &theme).header_actions(comment_dismiss_button(
                    id,
                    &theme,
                    cx,
                    move |reviewer, window, cx| {
                        reviewer.dismiss_comment(id, window, cx);
                    },
                ))
            })
            .collect::<Vec<_>>();
        (!cards.is_empty()).then(|| {
            div().pt_2().pb_1().child(CommentThread::new(("comment-thread", target.index()), &theme).cards(cards))
        })
    }

    fn render_composer(&self, target: MarkdownTargetId, cx: &mut Context<Self>) -> Option<Div> {
        let draft = self.session.draft().filter(|draft| draft.target() == target)?;
        let editor = self.editor.clone()?;
        let title = if draft.editing().is_some() { "Edit comment" } else { "Add a comment" };
        Some(
            div().pt_2().pb_1().child(
                CommentComposer::new(editor, title, &self.ui)
                    .on_cancel(Self::dispatch(ReviewCommand::Cancel, cx))
                    .on_submit(Self::dispatch(ReviewCommand::SubmitComment, cx)),
            ),
        )
    }

    fn code_line_text(&self, code: &CodeSource, index: usize) -> StyledText {
        let text = code.lines.get(index).cloned().unwrap_or_default();
        let highlighted = self
            .highlighter
            .borrow_mut()
            .with_theme(&self.theme.syntax)
            .highlight_document(code.source_id, LanguageHint::InfoString(&code.info), || code.text.clone())
            .unwrap_or_default();
        let spans = highlighted.line(index).unwrap_or_default().to_vec();
        StyledText::new(text).with_highlights(
            spans.into_iter().map(|span| (span.range, style::highlight_style(span.foreground, span.font_style))),
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct TextScale {
    size: f32,
    line_height: f32,
}

impl TextScale {
    const fn new(size: f32, line_height: f32) -> Self {
        Self { size, line_height }
    }
}

const fn heading_scale(level: u8) -> TextScale {
    match level {
        1 => TextScale::new(26.0, 34.0),
        2 => TextScale::new(21.0, 30.0),
        3 => TextScale::new(17.0, 26.0),
        _ => PROSE,
    }
}

fn first_line_center(kind: &RowKind) -> f32 {
    match kind {
        RowKind::Heading { level, .. } => BLOCK_PADDING_Y + heading_scale(*level).line_height / 2.0,
        RowKind::ListItem { .. } => 1.0 + PROSE.line_height / 2.0,
        RowKind::Quote { rows } => rows.first().map_or(PROSE.line_height / 2.0, |row| first_line_center(&row.kind)),
        RowKind::CodeHeader { .. } => CODE_HEADER_HEIGHT / 2.0,
        RowKind::CodeLine { .. } => CODE.line_height / 2.0,
        RowKind::TableRow { .. } => 6.0 + 10.0,
        RowKind::Paragraph { .. } | RowKind::Html { .. } | RowKind::Rule => BLOCK_PADDING_Y + PROSE.line_height / 2.0,
    }
}

fn indent(row: &DocumentRow) -> Rems {
    rem(LIST_INDENT * f32::from(row.depth))
}
