use crate::{
    DiffViewer, ViewerPane,
    annotation::add_comment_button,
    comment_editor::CommentEditor,
    comments::{CommentCard, CommentComposer, CommentCount, comment_dismiss_button},
    style,
    ui::{
        interface_size::{DecreaseInterfaceSize, IncreaseInterfaceSize, ResetInterfaceSize},
        prelude::{
            Button, ButtonVariant, ControlSize, IconButton, IconName, ListScrollbar, Typography,
            TypographyExt, rem,
        },
        tokens::{BAR_HEIGHT, CODE_ROW_HEIGHT},
    },
};
use clankerdiff_core::{
    DiffReviewCommand, DiffSide, DiffTone, FileDiff, GapInfo, PresentedCell, PresentedRow,
    RevealAmount, ReviewCommand, ReviewComment, RowKind,
};
use clankerdiff_theme::DiffPalette;
use gpui::{
    Action, AnyElement, Context, Div, Entity, HighlightStyle, MouseButton, Pixels, SharedString,
    Stateful, StyledText, Window, div, list, prelude::*, px,
};

const CHANGE_INDICATOR_WIDTH: f32 = 4.0;
const GUTTER_WIDTH: f32 = 52.0;
const FILE_DIVIDER_HEIGHT: f32 = 40.0;
const FILE_DIVIDER_INSET: f32 = 4.0;
const WHEEL_ROWS_PER_LINE: f32 = 2.0;

#[derive(Clone, Copy)]
enum FileDividerPlacement {
    Inline,
    Sticky,
}

impl DiffViewer {
    pub(crate) fn render_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let palette = self.theme().diff.clone();
        self.sync_diff_list();
        let list_state = self.diff_list_state.clone();
        list_state.set_wheel_line_height(self.diff_row_height() * WHEEL_ROWS_PER_LINE);
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
        let scrollbar = ListScrollbar::new("diff-scrollbar", list_state.clone(), self.ui_theme());
        let viewport = list_state.viewport_bounds();
        let divider_height = self.file_divider_height();
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
            .child(self.render_diff_toolbar(window, cx))
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
                    .child(scrollbar),
            )
    }

    pub(crate) fn file_divider_height(&self) -> Pixels {
        rem(FILE_DIVIDER_HEIGHT).to_pixels(self.rem_size())
    }

    fn render_diff_toolbar(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let palette = &self.theme().diff;
        let files = &self.document().files;
        let additions: usize = files.iter().map(FileDiff::additions).sum();
        let deletions: usize = files.iter().map(FileDiff::deletions).sum();
        let summary = match files.len() {
            1 => "1 file changed".to_owned(),
            count => format!("{count} files changed"),
        };
        div()
            .h(rem(BAR_HEIGHT))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
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
                    .child(div().typography(Typography::BodyStrong).child(summary))
                    .children(stat_badges(palette, additions, deletions)),
            )
            .children(self.render_collapse_all_button(cx))
            .child(self.render_interface_size_controls(window, cx))
    }

    fn render_collapse_all_button(&self, cx: &mut Context<Self>) -> Option<IconButton> {
        if self.document().files.is_empty() || self.session().source_view().is_some() {
            return None;
        }
        let (icon, label) = if self.session().all_files_collapsed() {
            (IconName::ChevronsUpDown, "Expand all files")
        } else {
            (IconName::ChevronsDownUp, "Collapse all files")
        };
        Some(
            IconButton::new("toggle-all-files-collapsed", icon, label, self.ui_theme()).on_click(
                cx.listener(|viewer, _, window, cx| {
                    viewer.handle_command(DiffReviewCommand::ToggleAllFilesCollapsed, window, cx);
                }),
            ),
        )
    }

    fn render_file_chevron(&self, file_index: usize, cx: &mut Context<Self>) -> IconButton {
        let (icon, label) = if self.session().is_file_collapsed(file_index) {
            (IconName::ChevronRight, "Expand file")
        } else {
            (IconName::ChevronDown, "Collapse file")
        };
        IconButton::new(
            format!("diff-file-collapse:{file_index}"),
            icon,
            label,
            self.ui_theme(),
        )
        .on_click(cx.listener(move |viewer, _, window, cx| {
            cx.stop_propagation();
            viewer.handle_command(
                DiffReviewCommand::ToggleFileCollapsed(Some(file_index)),
                window,
                cx,
            );
        }))
    }

    fn file_header_selected(&self, file_index: usize) -> bool {
        self.pane == ViewerPane::Diff
            && self.session().selected_row()
                == self
                    .presentation()
                    .file_range(file_index)
                    .map(|range| range.start)
    }

    fn collapsed_comment_count(&self, file_index: usize) -> usize {
        if !self.session().is_file_collapsed(file_index) {
            return 0;
        }
        let path = &self.document().files[file_index].path;
        self.review()
            .comments()
            .iter()
            .filter(|comment| &comment.anchor.path == path)
            .count()
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
        let selected = self.file_header_selected(file_index);
        let comments = self.collapsed_comment_count(file_index);
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
            .h(rem(FILE_DIVIDER_HEIGHT))
            .w_full()
            .px_2()
            .py(rem(FILE_DIVIDER_INSET))
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
                    .border_color(style::color(if selected {
                        palette.accent
                    } else {
                        palette.border
                    }))
                    .bg(style::color(surface))
                    .typography(Typography::Body)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .when(source_label.is_none(), |bar| {
                        bar.child(self.render_file_chevron(file_index, cx))
                    })
                    .child(
                        div()
                            .flex_shrink_0()
                            .typography(Typography::Code)
                            .text_color(style::color(style::file_status_color(
                                file.status,
                                palette,
                            )))
                            .child(file.status.code().to_string()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .typography(Typography::BodyStrong)
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
                    })
                    .when(comments != 0, |bar| {
                        bar.child(
                            div()
                                .debug_selector(move || {
                                    format!("diff-file-comment-count:{file_index}")
                                })
                                .flex_shrink_0()
                                .child(CommentCount::new(comments, &self.ui_theme())),
                        )
                    }),
            )
    }

    fn render_interface_size_controls(&self, window: &Window, _cx: &mut Context<Self>) -> Div {
        let theme = self.ui_theme();
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                IconButton::new(
                    "decrease-interface-size",
                    IconName::Minus,
                    "Decrease interface size",
                    theme,
                )
                .on_click(|_, window, cx| {
                    window.dispatch_action(DecreaseInterfaceSize.boxed_clone(), cx);
                }),
            )
            .child(
                Button::new(
                    "reset-interface-size",
                    format!("{:.0}", f32::from(window.rem_size())),
                    theme,
                )
                .variant(ButtonVariant::Secondary)
                .size(ControlSize::Small)
                .aria_label("Reset interface size")
                .on_click(|_, window, cx| {
                    window.dispatch_action(ResetInterfaceSize.boxed_clone(), cx);
                }),
            )
            .child(
                IconButton::new(
                    "increase-interface-size",
                    IconName::Plus,
                    "Increase interface size",
                    theme,
                )
                .on_click(|_, window, cx| {
                    window.dispatch_action(IncreaseInterfaceSize.boxed_clone(), cx);
                }),
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
            .h(rem(CODE_ROW_HEIGHT))
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
                    .w(rem(CHANGE_INDICATOR_WIDTH + GUTTER_WIDTH))
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .when(expandable, |gutter| {
                        gutter.child(
                            IconButton::new(
                                ("diff-gap-expand", index),
                                IconName::ChevronsUpDown,
                                info.map_or_else(
                                    || "Expand unchanged lines".to_owned(),
                                    GapInfo::message,
                                ),
                                self.ui_theme(),
                            )
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
                gap.child(line().w(rem(12.0)).flex_shrink_0()).child(
                    div()
                        .px_2()
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .typography(Typography::Caption)
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
            .h(rem(CODE_ROW_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .px_3()
            .typography(Typography::Code)
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
        let row_height = rem(CODE_ROW_HEIGHT);
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
                .min_h(rem(CODE_ROW_HEIGHT))
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
            .min_h(rem(CODE_ROW_HEIGHT))
            .typography(Typography::Code)
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
                    .w(rem(CHANGE_INDICATOR_WIDTH))
                    .min_h(rem(CODE_ROW_HEIGHT))
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
                value.child(CommentCount::new(comments, &self.ui_theme()))
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
            .w(rem(GUTTER_WIDTH))
            .min_h(rem(CODE_ROW_HEIGHT))
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
                            &self.ui_theme(),
                            offset == last_comment,
                        )
                        .header_actions(comment_dismiss_button(
                            comment.id,
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

fn stat_badges(palette: &DiffPalette, additions: usize, deletions: usize) -> [Div; 2] {
    [
        div()
            .flex_shrink_0()
            .typography(Typography::Code)
            .text_color(style::color(palette.addition))
            .child(format!("+{additions}")),
        div()
            .flex_shrink_0()
            .typography(Typography::Code)
            .text_color(style::color(palette.deletion))
            .child(format!("−{deletions}")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use clankerdiff_core::testing::DocumentBuilder;

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
