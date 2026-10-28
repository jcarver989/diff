use super::{
    MarkdownReviewer,
    content::{BLOCK_PADDING_Y, CODE, CODE_HEADER_HEIGHT, MarkdownContent, PROSE, heading_scale},
    rows::{DocumentRow, RowKind},
};
use crate::{
    annotation::add_comment_button,
    comments::{CommentCard, CommentComposer, CommentThread, comment_dismiss_button},
    ui::prelude::{EmptyState, NoticeTone, rem},
};
use clankerdiff_core::ReviewCommand;
use clankerdiff_markdown::{MarkdownFocusPane, MarkdownTargetId};
use gpui::{AnyElement, Context, Div, SharedString, div, prelude::*, px};

const COLUMN_WIDTH: f32 = 780.0;
const GUTTER_WIDTH: f32 = 28.0;
const ADD_BUTTON_SIZE: f32 = 18.0;

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
            .children({
                let mut highlighter = self.highlighter.borrow_mut();
                let mut content = MarkdownContent {
                    theme: &self.theme,
                    ui: &self.ui,
                    inline: self.inline,
                    highlighter: &mut highlighter,
                    copy: self.capabilities.clipboard,
                    compact: false,
                };
                self.rows
                    .iter()
                    .enumerate()
                    .map(|(index, row)| self.render_row(&mut content, index, row, cx))
                    .collect::<Vec<_>>()
            })
    }

    fn render_row(
        &self,
        content: &mut MarkdownContent<'_>,
        index: usize,
        row: &DocumentRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = self.ui.colors;
        let selected = row.target.is_some() && row.target == self.session.selected_target();
        let group: SharedString = format!("markdown-row-{index}").into();
        let block = content
            .block(row)
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
                    .child(div().flex_1().min_w_0().pl(content.indent(row)).flex().flex_col().child(block).when_some(
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
