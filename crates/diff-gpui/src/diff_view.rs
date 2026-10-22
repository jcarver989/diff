use crate::{
    DiffViewer, ViewerPane,
    annotation::add_comment_button,
    comment_editor::CommentEditor,
    style,
    ui::{
        comments::{CommentCard, CommentComposer, CommentCount, comment_dismiss_button},
        prelude::{Button, ButtonVariant, ControlSize, icon_button},
    },
};
use clankerdiff_core::{
    DiffReviewCommand, DiffSide, DiffTone, FileDiff, GapInfo, PresentedCell, PresentedRow,
    RevealAmount, ReviewCommand, ReviewComment, RowKind,
};
use clankerdiff_theme::DiffPalette;
use gpui::{
    AnyElement, Context, Div, DragMoveEvent, Empty, Entity, HighlightStyle, ListState, MouseButton,
    MouseDownEvent, Pixels, SharedString, Stateful, StyledText, Window, div, list, point,
    prelude::*, px,
};
use std::cell::Cell;

const CHANGE_INDICATOR_WIDTH: f32 = 5.0;
const GUTTER_WIDTH: f32 = 54.0;
const HEADER_HEIGHT: f32 = 52.0;
const FILE_DIVIDER_INSET: f32 = 4.0;
const SCROLLBAR_WIDTH: f32 = 20.0;
const MIN_THUMB_HEIGHT: f32 = 30.0;

#[derive(Clone, Copy)]
enum FileDividerPlacement {
    Inline,
    Sticky,
}

struct DiffScrollbarDrag {
    thumb_offset: Cell<Pixels>,
    list_state: ListState,
    started: Cell<bool>,
}

impl DiffScrollbarDrag {
    fn start(&self, thumb_offset: Pixels) {
        self.thumb_offset.set(thumb_offset);
        self.list_state.scrollbar_drag_started();
        self.started.set(true);
    }
}

impl Drop for DiffScrollbarDrag {
    fn drop(&mut self) {
        if self.started.get() {
            self.list_state.scrollbar_drag_ended();
        }
    }
}

impl DiffViewer {
    pub(crate) fn render_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let palette = self.theme().diff.clone();
        self.sync_diff_list();
        let list_state = self.diff_list_state.clone();
        let Some(file_index) = self.active_file() else {
            return div()
                .debug_selector(|| "diff-pane".to_owned())
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(style::color(palette.muted))
                .child("No changes to review");
        };
        list_state.set_scroll_handler({
            let viewer = cx.entity().downgrade();
            move |_, _, cx| {
                viewer.update(cx, |_, cx| cx.notify()).ok();
            }
        });
        let scrollbar = self.render_scrollbar(&list_state, window, cx);
        let viewport = list_state.viewport_bounds();
        let divider_height = px(self.file_divider_height());
        let sticky_top = self
            .presentation()
            .file_range(file_index)
            .and_then(|range| self.list_item(range.end))
            .and_then(|next| list_state.bounds_for_item(next))
            .map_or(px(0.0), |next| {
                (next.top() - viewport.top() - divider_height).min(px(0.0))
            });
        let start = self.session().visible_range().start;
        let list = list(
            list_state,
            cx.processor(move |viewer, offset: usize, _window, cx| {
                viewer.render_visible_row(start + offset, cx)
            }),
        )
        .size_full();

        div()
            .debug_selector(|| "diff-pane".to_owned())
            .flex_1()
            .h_full()
            .when(self.pane == ViewerPane::Diff, |pane| {
                pane.border_l_1().border_color(style::color(palette.accent))
            })
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_diff_toolbar(cx))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .overflow_hidden()
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_hidden()
                            .child(list)
                            .child(div().absolute().top(sticky_top).left_0().right_0().child(
                                self.render_file_divider(
                                    file_index,
                                    FileDividerPlacement::Sticky,
                                    cx,
                                ),
                            )),
                    )
                    .children(scrollbar),
            )
    }

    pub(crate) fn file_divider_height(&self) -> f32 {
        (self.diff_row_height() * 1.5).round() + FILE_DIVIDER_INSET * 2.0
    }

    fn render_diff_toolbar(&self, cx: &mut Context<Self>) -> Div {
        let palette = &self.theme().diff;
        let files = &self.document().files;
        let additions: usize = files.iter().map(FileDiff::additions).sum();
        let deletions: usize = files.iter().map(FileDiff::deletions).sum();
        let summary = match files.len() {
            1 => "1 file changed".to_owned(),
            count => format!("{count} files changed"),
        };
        div()
            .h(px(HEADER_HEIGHT))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .border_b_1()
            .border_color(style::color(palette.border))
            .child(
                div()
                    .debug_selector(|| "diff-summary".to_owned())
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .gap_2()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(summary))
                    .children(stat_badges(palette, additions, deletions)),
            )
            .child(self.render_font_controls(cx))
    }

    fn render_file_divider(
        &self,
        file_index: usize,
        placement: FileDividerPlacement,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let palette = &self.theme().diff;
        let surface = self.theme().ui.surface;
        let file = &self.document().files[file_index];
        let directory = file
            .path
            .directory()
            .map(|directory| format!("{directory}/"));
        let renamed_from = file.renamed_from().map(|old| format!("← {old}"));
        let source_label = self.session().source_view_label();
        let (id, selector) = match placement {
            FileDividerPlacement::Inline => ("diff-file-header", "diff-file-header"),
            FileDividerPlacement::Sticky => ("diff-sticky-file-header", "diff-active-file"),
        };
        div()
            .id((id, file_index))
            .debug_selector(move || format!("{selector}:{file_index}"))
            .h(px(self.file_divider_height()))
            .w_full()
            .px_2()
            .py(px(FILE_DIVIDER_INSET))
            .bg(style::color(palette.background))
            .cursor_pointer()
            .on_click(cx.listener(move |viewer, _, window, cx| {
                viewer.handle_command(DiffReviewCommand::SelectFile(file_index), window, cx);
            }))
            .child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .rounded_md()
                    .border_1()
                    .border_color(style::color(palette.border))
                    .bg(style::color(surface))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(style::color(style::file_status_color(
                                file.status,
                                palette,
                            )))
                            .child(file.status.code().to_string()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(style::color(palette.foreground))
                            .child(file.path.file_name().to_owned()),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .gap_2()
                            .overflow_hidden()
                            .text_color(style::color(palette.muted))
                            .children(directory)
                            .children(renamed_from),
                    )
                    .children(source_label.map(|label| {
                        div()
                            .debug_selector(|| "source-mode-header".to_owned())
                            .flex_shrink_0()
                            .text_color(style::color(palette.muted))
                            .child(label)
                    }))
                    .when(source_label.is_none(), |bar| {
                        bar.children(stat_badges(palette, file.additions(), file.deletions()))
                    }),
            )
    }

    fn render_font_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.ui_theme();
        div()
            .mr_4()
            .flex()
            .items_center()
            .gap_1()
            .text_size(px(self.metadata_font_size()))
            .child(
                icon_button("decrease-font-size", "A−", "Decrease font size", theme)
                    .on_click(cx.listener(|viewer, _, _, cx| viewer.adjust_font_size(-1.0, cx))),
            )
            .child(
                Button::new("reset-font-size", format!("{:.0}", self.font_size()), theme)
                    .variant(ButtonVariant::Secondary)
                    .size(ControlSize::Small)
                    .aria_label("Reset font size")
                    .on_click(cx.listener(|viewer, _, _, cx| viewer.reset_font_size(cx))),
            )
            .child(
                icon_button("increase-font-size", "A+", "Increase font size", theme)
                    .on_click(cx.listener(|viewer, _, _, cx| viewer.adjust_font_size(1.0, cx))),
            )
    }

    fn render_scrollbar(
        &self,
        list_state: &ListState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let max_offset = list_state.max_offset_for_scrollbar().y;
        let viewport_height = list_state.viewport_bounds().size.height;
        if viewport_height <= px(0.0) {
            if list_state.item_count() != 0 {
                cx.on_next_frame(window, |_, _, cx| cx.notify());
            }
            return None;
        }
        if max_offset <= px(0.0) {
            return None;
        }

        let current_offset = -list_state.scroll_px_offset_for_scrollbar().y;
        let thumb_height = scrollbar_thumb_height(viewport_height);
        let track_space = viewport_height - thumb_height;
        let scroll_fraction = (current_offset / max_offset).clamp(0.0, 1.0);
        let thumb_top = track_space * scroll_fraction;
        let palette = &self.theme().diff;
        let drag = DiffScrollbarDrag {
            thumb_offset: Cell::new(px(0.0)),
            list_state: list_state.clone(),
            started: Cell::new(false),
        };

        let click_state = list_state.clone();
        let drag_state = list_state.clone();
        Some(
            div()
                .id("diff-scrollbar")
                .debug_selector(|| "diff-scrollbar".to_owned())
                .w(px(SCROLLBAR_WIDTH))
                .h_full()
                .flex_shrink_0()
                .relative()
                .bg(style::color(palette.background))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                        set_scrollbar_from_pointer(
                            &click_state,
                            event.position.y,
                            thumb_height / 2.0,
                        );
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_drag_move(cx.listener(
                    move |_, event: &DragMoveEvent<DiffScrollbarDrag>, _, cx| {
                        let thumb_offset = event.drag(cx).thumb_offset.get();
                        set_scrollbar_from_pointer(
                            &drag_state,
                            event.event.position.y,
                            thumb_offset,
                        );
                        cx.notify();
                    },
                ))
                .child(
                    div()
                        .id("diff-scrollbar-thumb")
                        .debug_selector(|| "diff-scrollbar-thumb".to_owned())
                        .absolute()
                        .top(thumb_top)
                        .w_full()
                        .h(thumb_height)
                        .rounded_sm()
                        .cursor_pointer()
                        .bg(style::color(palette.muted))
                        .hover(|thumb| thumb.bg(style::color(palette.accent)))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_drag(drag, |drag, click_offset, _, cx| {
                            drag.start(click_offset.y);
                            cx.new(|_| Empty)
                        }),
                )
                .into_any_element(),
        )
    }

    fn render_visible_row(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.presentation().row(index).cloned() else {
            return div().into_any_element();
        };
        self.render_presented_row(index, &row, cx)
    }

    fn render_gap_row(
        &self,
        index: usize,
        row: &PresentedRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = &self.theme().diff;
        let info = self.presentation().gap_info(index);
        let expandable = info.is_some_and(GapInfo::is_expandable);
        let selected =
            self.pane == ViewerPane::Diff && self.session().selected_row() == Some(index);
        let line_color = style::color(if selected {
            palette.accent
        } else {
            palette.border
        });
        let line = || div().h(px(1.0)).bg(line_color);
        div()
            .id(("diff-row", row.id.0))
            .debug_selector(move || format!("diff-gap:{index}"))
            .h(px(self.diff_row_height()))
            .w_full()
            .flex()
            .items_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |viewer, _, window, cx| {
                    viewer.handle_command(DiffReviewCommand::SelectRow(index), window, cx);
                }),
            )
            .child(
                div()
                    .w(px(CHANGE_INDICATOR_WIDTH + GUTTER_WIDTH))
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .when(expandable, |gutter| {
                        gutter.child(
                            icon_button(
                                ("diff-gap-expand", index),
                                "↕",
                                "Expand unchanged lines",
                                self.ui_theme(),
                            )
                            .tooltip(info.map(GapInfo::message).unwrap_or_default())
                            .on_click(cx.listener(
                                move |viewer, _, window, cx| {
                                    if viewer.handle_command(
                                        DiffReviewCommand::SelectRow(index),
                                        window,
                                        cx,
                                    ) {
                                        viewer.handle_command(
                                            DiffReviewCommand::RevealGap(RevealAmount::Step),
                                            window,
                                            cx,
                                        );
                                    }
                                },
                            )),
                        )
                    }),
            )
            .when_some(info.filter(|_| selected), |gap, info| {
                gap.child(line().w(px(12.0)).flex_shrink_0()).child(
                    div()
                        .px_2()
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .text_size(px(self.metadata_font_size()))
                        .text_color(style::color(palette.muted))
                        .child(info.hint()),
                )
            })
            .child(line().flex_1())
            .into_any_element()
    }

    fn render_meta_row(&self, row: &PresentedRow) -> AnyElement {
        let palette = &self.theme().diff;
        let text: SharedString = row
            .primary_cell()
            .map_or_else(SharedString::default, |cell| cell.text.to_string().into());
        div()
            .id(("diff-row", row.id.0))
            .h(px(self.diff_row_height()))
            .w_full()
            .flex()
            .items_center()
            .px_3()
            .bg(style::color(palette.selection))
            .border_b_1()
            .border_color(style::color(palette.border))
            .text_color(style::color(palette.accent))
            .child(text)
            .into_any_element()
    }

    #[allow(clippy::too_many_lines)]
    fn render_presented_row(
        &mut self,
        index: usize,
        row: &PresentedRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row_height = px(self.diff_row_height());
        match row.kind {
            RowKind::FileHeader => {
                return self
                    .render_file_divider(row.file_index, FileDividerPlacement::Inline, cx)
                    .into_any_element();
            }
            RowKind::ExpandGap => return self.render_gap_row(index, row, cx),
            RowKind::Meta => return self.render_meta_row(row),
            RowKind::Code | RowKind::ExpandedContext => {}
        }

        let split = self.layout().is_split();
        let mut cells = div().min_h(row_height).w_full().flex();
        if split {
            cells = cells
                .child(self.render_cell(index, row, DiffSide::Old, row.left.as_ref(), true, cx))
                .child(self.render_cell(index, row, DiffSide::New, row.right.as_ref(), false, cx));
        } else {
            let side = if row.right.is_some() {
                DiffSide::New
            } else {
                DiffSide::Old
            };
            cells = cells.child(self.render_cell(index, row, side, row.primary_cell(), false, cx));
        }

        let active_editor = self
            .session()
            .draft()
            .filter(|draft| self.presentation().row_shows_anchor(row, draft.anchor()))
            .and_then(|_| {
                self.comment_target
                    .as_ref()
                    .filter(|target| target.row_index == index)
                    .and_then(|target| {
                        self.comment_editor
                            .clone()
                            .map(|editor| (target.side, editor))
                    })
            });

        let mut element = div()
            .id(("diff-row", row.id.0))
            .min_h(row_height)
            .w_full()
            .flex()
            .flex_col()
            .child(cells);
        if split {
            let old_comments = self.comments_for_row(row, Some(DiffSide::Old));
            let new_comments = self.comments_for_row(row, Some(DiffSide::New));
            if !old_comments.is_empty() || !new_comments.is_empty() || active_editor.is_some() {
                element = element.child(self.render_split_annotations(
                    index,
                    row,
                    old_comments,
                    new_comments,
                    active_editor,
                    cx,
                ));
            }
        } else {
            let comments = self.comments_for_row(row, None);
            if !comments.is_empty() {
                element = element.child(self.render_comment_thread(index, comments, cx));
            }
            if let Some((side, editor)) = active_editor {
                element = element.child(self.render_comment_dialog(index, row, side, editor, cx));
            }
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
        let palette = self.theme().diff.clone();
        let split = self.layout().is_split();
        let border_color = style::color(palette.border);
        let Some(cell) = cell else {
            return div()
                .w_1_2()
                .min_h(px(self.diff_row_height()))
                .bg(style::color(palette.background))
                .when(left, |value| value.border_r_1().border_color(border_color))
                .into_any_element();
        };

        let text: SharedString = cell.text.to_string().into();
        let highlights = self.highlight_cell(row, cell);
        let styled = StyledText::new(text).with_highlights(highlights.iter().map(|span| {
            let style: HighlightStyle = style::highlight_style(span.foreground, span.font_style);
            (span.range.clone(), style)
        }));
        let tone = cell.tone;
        let tone_background = palette.tone(tone).background;
        let commentable = cell.patch_source.is_some();
        let comments = self.comments_on(index, cell);
        let hover_group: SharedString = format!("comment-cell-{index}-{side:?}").into();
        let selected =
            self.session().selected_row() == Some(index) && self.session().selected_side() == side;

        let mut element = div()
            .id((if left { "old-cell" } else { "new-cell" }, index))
            .group(hover_group.clone())
            .when(split, gpui::Styled::w_1_2)
            .when(!split, gpui::Styled::w_full)
            .min_h(px(self.diff_row_height()))
            .flex()
            .items_start()
            .overflow_hidden()
            .bg(style::color(if selected {
                palette.selection
            } else {
                tone_background
            }))
            .when(left, |value| value.border_r_1().border_color(border_color))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |viewer, _, window, cx| {
                    viewer.select_diff_cell(index, side, window, cx);
                }),
            );
        if commentable {
            element = element.hover(|hover| hover.bg(style::color(palette.selection)));
        }
        let indicator_color = match tone {
            DiffTone::Added => Some(palette.addition),
            DiffTone::Removed => Some(palette.deletion),
            DiffTone::Context | DiffTone::Meta => None,
        };
        element
            .child(
                div()
                    .w(px(CHANGE_INDICATOR_WIDTH))
                    .min_h(px(self.diff_row_height()))
                    .self_stretch()
                    .flex_shrink_0()
                    .when_some(indicator_color, |indicator, color| {
                        indicator.bg(style::color(color))
                    }),
            )
            .child(self.render_comment_gutter(index, side, cell, commentable, hover_group, cx))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_normal()
                    .child(styled),
            )
            .when(comments != 0, |value| {
                value.child(CommentCount::new(
                    comments,
                    self.metadata_font_size(),
                    &self.ui_theme(),
                ))
            })
            .into_any_element()
    }

    fn render_comment_gutter(
        &self,
        index: usize,
        side: DiffSide,
        cell: &PresentedCell,
        commentable: bool,
        hover_group: SharedString,
        cx: &mut Context<Self>,
    ) -> Div {
        let palette = &self.theme().diff;
        let foreground = match cell.tone {
            DiffTone::Added => palette.addition,
            DiffTone::Removed => palette.deletion,
            DiffTone::Context | DiffTone::Meta => palette.gutter,
        };
        div()
            .w(px(GUTTER_WIDTH))
            .min_h(px(20.0))
            .flex_shrink_0()
            .relative()
            .text_color(style::color(foreground))
            .text_center()
            .child(
                cell.line_number()
                    .map_or_else(String::new, |number| number.to_string()),
            )
            .when(commentable, |gutter| {
                gutter.child(
                    add_comment_button(
                        format!("add-comment-{index}-{side:?}"),
                        hover_group,
                        palette,
                    )
                    .on_click(cx.listener(move |viewer, _, window, cx| {
                        cx.stop_propagation();
                        viewer.begin_comment(index, side, window, cx);
                    })),
                )
            })
    }

    fn comments_for_row(&self, row: &PresentedRow, side: Option<DiffSide>) -> Vec<ReviewComment> {
        let mut comments = Vec::new();
        for cell in row.cells() {
            let Some(anchor) = self.presentation().cell_anchor(row, cell) else {
                continue;
            };
            if side.is_none_or(|side| anchor.side == side) {
                comments.extend(self.review().comments_for_anchor(&anchor).cloned());
            }
        }
        comments
    }

    fn render_split_annotations(
        &mut self,
        index: usize,
        row: &PresentedRow,
        old_comments: Vec<ReviewComment>,
        new_comments: Vec<ReviewComment>,
        active_editor: Option<(DiffSide, Entity<CommentEditor>)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.theme().diff.clone();
        let old_editor = active_editor
            .as_ref()
            .filter(|(side, _)| *side == DiffSide::Old)
            .map(|(_, editor)| editor.clone());
        let new_editor = active_editor
            .filter(|(side, _)| *side == DiffSide::New)
            .map(|(_, editor)| editor);

        let old_column = div()
            .w_1_2()
            .min_w_0()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(style::color(palette.border))
            .when(!old_comments.is_empty(), |column| {
                column.child(self.render_comment_thread(index, old_comments, cx))
            })
            .when_some(old_editor, |column, editor| {
                column.child(self.render_comment_dialog(index, row, DiffSide::Old, editor, cx))
            });
        let new_column = div()
            .w_1_2()
            .min_w_0()
            .flex()
            .flex_col()
            .when(!new_comments.is_empty(), |column| {
                column.child(self.render_comment_thread(index, new_comments, cx))
            })
            .when_some(new_editor, |column, editor| {
                column.child(self.render_comment_dialog(index, row, DiffSide::New, editor, cx))
            });

        div()
            .id(("split-comment-annotations", index))
            .w_full()
            .flex()
            .items_stretch()
            .bg(style::color(palette.background))
            .child(old_column)
            .child(new_column)
            .into_any_element()
    }

    fn render_comment_thread(
        &self,
        index: usize,
        comments: Vec<ReviewComment>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.theme().diff.clone();
        let last_comment = comments.len().saturating_sub(1);
        div()
            .id(("comment-thread", index))
            .w_full()
            .px_3()
            .pb_3()
            .bg(style::color(palette.background))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .rounded_md()
                    .border_1()
                    .border_color(style::color(palette.border))
                    .overflow_hidden()
                    .children(comments.into_iter().enumerate().map(|(offset, comment)| {
                        let side = if comment.anchor.side == DiffSide::Old {
                            "old"
                        } else {
                            "new"
                        };
                        let line = comment
                            .anchor
                            .line_number()
                            .map_or_else(|| "line".to_owned(), |line| format!("line {line}"));
                        CommentCard::new(
                            comment.id,
                            format!("Your comment on {side} {line}"),
                            comment.body,
                            self.metadata_font_size(),
                            &self.ui_theme(),
                            offset == last_comment,
                        )
                        .header_actions(comment_dismiss_button(
                            comment.id,
                            self.font_size(),
                            &self.ui_theme(),
                            cx,
                            move |viewer, window, cx| {
                                viewer.dismiss_comment(comment.id, window, cx);
                            },
                        ))
                    })),
            )
            .into_any_element()
    }

    fn render_comment_dialog(
        &mut self,
        index: usize,
        row: &PresentedRow,
        side: DiffSide,
        editor: Entity<CommentEditor>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.theme().diff.clone();
        let line = row
            .cell(side)
            .and_then(PresentedCell::line_number)
            .map_or_else(|| "line".to_owned(), |number| format!("line {number}"));
        let side_label = if side == DiffSide::Old { "old" } else { "new" };
        let can_submit = !editor.read(cx).is_blank();
        let theme = self.ui_theme();
        let cancel_button = Button::new(("cancel-comment", index), "Cancel", theme)
            .size(ControlSize::Small)
            .on_click(cx.listener(|viewer, _, window, cx| {
                viewer.handle_command(ReviewCommand::Cancel, window, cx);
            }));
        let submit_button = Button::new(("submit-comment", index), "Add comment", theme)
            .variant(ButtonVariant::Primary)
            .size(ControlSize::Small)
            .disabled(!can_submit)
            .on_click(cx.listener(|viewer, _, window, cx| {
                viewer.handle_command(ReviewCommand::SubmitComment, window, cx);
            }));

        div()
            .id(("comment-dialog", index))
            .w_full()
            .px_3()
            .pb_3()
            .bg(style::color(palette.background))
            .child(CommentComposer::new(
                editor,
                format!("Add a comment on {side_label} {line}"),
                &theme,
                cancel_button,
                submit_button,
            ))
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

fn scrollbar_thumb_height(viewport_height: Pixels) -> Pixels {
    // Variable-list content height is estimated until every row has been measured. A fixed thumb
    // avoids visibly shrinking as wrapped rows are discovered while preserving useful drag space.
    px(MIN_THUMB_HEIGHT).min(viewport_height / 2.0)
}

fn set_scrollbar_from_pointer(list_state: &ListState, pointer_y: Pixels, thumb_offset: Pixels) {
    let viewport = list_state.viewport_bounds();
    let max_offset = list_state.max_offset_for_scrollbar().y;
    let thumb_height = scrollbar_thumb_height(viewport.size.height);
    let track_space = viewport.size.height - thumb_height;
    if max_offset <= px(0.0) || track_space <= px(0.0) {
        return;
    }

    let thumb_top = (pointer_y - viewport.origin.y - thumb_offset).clamp(px(0.0), track_space);
    list_state.set_offset_from_scrollbar(point(px(0.0), -max_offset * (thumb_top / track_space)));
}

fn stat_badges(palette: &DiffPalette, additions: usize, deletions: usize) -> [Div; 2] {
    [
        div()
            .flex_shrink_0()
            .text_color(style::color(palette.addition))
            .child(format!("+{additions}")),
        div()
            .flex_shrink_0()
            .text_color(style::color(palette.deletion))
            .child(format!("−{deletions}")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use clankerdiff_core::testing::DocumentBuilder;

    #[test]
    fn scrollbar_drag_releases_frozen_height_on_drop() {
        let list_state = ListState::new(10, gpui::ListAlignment::Top, px(0.0));
        let drag = DiffScrollbarDrag {
            thumb_offset: Cell::new(px(0.0)),
            list_state: list_state.clone(),
            started: Cell::new(false),
        };

        drag.start(px(4.0));
        assert!(list_state.is_scrollbar_dragging());
        assert_eq!(drag.thumb_offset.get(), px(4.0));

        drop(drag);
        assert!(!list_state.is_scrollbar_dragging());
    }

    #[test]
    fn scrollbar_thumb_stays_stable_while_rows_are_measured() {
        assert_eq!(scrollbar_thumb_height(px(100.0)), px(30.0));
    }

    #[test]
    fn scrollbar_thumb_leaves_drag_space_in_a_short_viewport() {
        assert_eq!(scrollbar_thumb_height(px(20.0)), px(10.0));
    }

    #[test]
    fn submitted_comments_remain_attached_to_their_row() {
        let mut viewer = DiffViewer::new(
            DocumentBuilder::new()
                .changed("src/main.rs", "old\n", "new\n")
                .build(),
        );
        let row_index = (0..viewer.presentation().row_count())
            .find(|index| viewer.presentation().is_commentable(*index))
            .expect("a changed document has a commentable row");
        let row = viewer
            .presentation()
            .row(row_index)
            .expect("selected presentation row exists")
            .clone();
        let cell = row.primary_cell().expect("commentable row has a cell");
        let anchor = viewer
            .presentation()
            .cell_anchor(&row, cell)
            .expect("commentable cell has an anchor");
        let side = anchor.side;
        viewer
            .session_mut()
            .review_mut()
            .add_comment(anchor, "Keep this visible");

        let comments = viewer.comments_for_row(&row, None);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].body, "Keep this visible");
        assert_eq!(viewer.comments_for_row(&row, Some(side)).len(), 1);
        assert!(
            viewer
                .comments_for_row(&row, Some(side.opposite()))
                .is_empty()
        );
    }
}
