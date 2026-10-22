use crate::{
    DiffViewer, ViewerPane,
    style::{color, file_status_color},
};
use clankerdiff_core::{DiffReviewCommand, FileTree, FileTreeEntry, StageState};
use gpui::{
    ClickEvent, Context, Div, Empty, PathBuilder, Role, Stateful, canvas, div, point, prelude::*,
    px,
};
use gpui_base::{Checkbox, CheckboxIndicator, CheckboxState};

const HEADER_HEIGHT: f32 = 52.0;
const INDENT_WIDTH: f32 = 16.0;
const DISCLOSURE_WIDTH: f32 = 14.0;
const STAGE_CHECKBOX_WIDTH: f32 = 18.0;
const RESIZE_HANDLE_WIDTH: f32 = 8.0;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SidebarResizeDrag;

fn sidebar_row(depth: usize, height: f32) -> Div {
    div()
        .h(px(height))
        .flex()
        .items_center()
        .gap_2()
        .pr_3()
        .pl(px(12.0
            + f32::from(u16::try_from(depth).unwrap_or(u16::MAX))
                * INDENT_WIDTH))
        .cursor_pointer()
}

impl DiffViewer {
    fn stage_checkbox(&self, entry: &FileTreeEntry, cx: &mut Context<Self>) -> Checkbox {
        let palette = &self.theme().diff;
        let checkbox_state = match FileTree::stage_state_for(self.document(), &entry.path) {
            StageState::Staged => CheckboxState::Checked,
            StageState::PartiallyStaged => CheckboxState::Indeterminate,
            StageState::Unstaged => CheckboxState::Unchecked,
        };
        let viewer = cx.entity().downgrade();
        let path = entry.path.clone();
        let label = format!("Toggle staging for {path}");
        let mark_color = color(palette.foreground);
        let (checkbox_id, id) = if let Some(index) = entry.file {
            (
                format!("diff-file-checkbox:{path}"),
                format!("diff-file-checkbox:{index}"),
            )
        } else {
            let id = format!("diff-directory-checkbox:{path}");
            (id.clone(), id)
        };
        Checkbox::new(checkbox_id)
            .debug_selector(move || id.clone())
            .state(checkbox_state)
            .key_context("StageCheckbox")
            .accessibility_label(label)
            .w(px(STAGE_CHECKBOX_WIDTH))
            .h(px(STAGE_CHECKBOX_WIDTH))
            .flex_shrink_0()
            .cursor_pointer()
            .rounded(px(3.0))
            .border_1()
            .border_color(color(palette.muted))
            .bg(color(palette.background))
            .hover(|style| style.border_color(color(palette.accent)))
            .focus_visible(|style| style.border_color(color(palette.accent)))
            .on_change(move |_, _, window, cx| {
                let _ = viewer.update(cx, |viewer, cx| {
                    viewer.toggle_stage_path(&path, window, cx);
                });
                cx.stop_propagation();
            })
            .child(
                CheckboxIndicator::new()
                    .state(checkbox_state)
                    .size_full()
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, (), window, _| {
                                if checkbox_state == CheckboxState::Unchecked {
                                    return;
                                }
                                let mut path = PathBuilder::stroke(px(2.0));
                                let at = |x: f32, y: f32| {
                                    point(
                                        bounds.left() + bounds.size.width * x,
                                        bounds.top() + bounds.size.height * y,
                                    )
                                };
                                if checkbox_state == CheckboxState::Checked {
                                    path.move_to(at(0.2, 0.5));
                                    path.line_to(at(0.43, 0.73));
                                    path.line_to(at(0.8, 0.27));
                                } else {
                                    path.move_to(at(0.2, 0.5));
                                    path.line_to(at(0.8, 0.5));
                                }
                                if let Ok(path) = path.build() {
                                    window.paint_path(path, mark_color);
                                }
                            },
                        )
                        .size_full(),
                    ),
            )
    }

    pub(crate) fn render_sidebar(&self, cx: &mut Context<Self>) -> Div {
        let palette = &self.theme().diff;
        let mut rows = div()
            .id("diff-files")
            .debug_selector(|| "diff-files".to_owned())
            .role(Role::Tree)
            .flex_1()
            .overflow_y_scroll()
            .track_scroll(&self.sidebar_scroll_handle)
            .py_2();

        let selected = self.sidebar_tree.selected_position();
        for (position, entry) in self.sidebar_tree.entries().iter().enumerate() {
            let selected = selected == Some(position);
            match entry.file {
                None => rows = rows.child(self.render_directory_row(position, entry, selected, cx)),
                Some(index) => {
                    if let Some(row) = self.render_file_row(index, entry, selected, cx) {
                        rows = rows.child(row);
                    }
                }
            }
        }

        div()
            .debug_selector(|| "diff-sidebar".to_owned())
            .w(px(self.sidebar_width()))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(HEADER_HEIGHT))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .px_4()
                    .border_b_1()
                    .border_color(color(palette.border))
                    .text_size(px(self.heading_font_size()))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(format!("CHANGED FILES  {}", self.document().files.len())),
            )
            .when(self.pane == ViewerPane::Files, |pane| {
                pane.border_r_1().border_color(color(palette.accent))
            })
            .child(rows)
    }

    pub(crate) fn render_sidebar_resize_handle(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let palette = &self.theme().diff;
        div()
            .id("sidebar-resize-container")
            .relative()
            .h_full()
            .w(px(1.0))
            .flex_shrink_0()
            .bg(color(palette.border))
            .child(
                div()
                    .id("sidebar-resize-handle")
                    .absolute()
                    .left(px(-RESIZE_HANDLE_WIDTH / 2.0))
                    .w(px(RESIZE_HANDLE_WIDTH))
                    .h_full()
                    .cursor_col_resize()
                    .block_mouse_except_scroll()
                    .hover(|handle| handle.bg(color(palette.accent)))
                    .on_click(cx.listener(|viewer, event: &ClickEvent, _, cx| {
                        if event.click_count() >= 2 {
                            viewer.reset_sidebar_width(cx);
                        }
                        cx.stop_propagation();
                    }))
                    .on_drag(SidebarResizeDrag, |_, _, _, cx| cx.new(|_| Empty)),
            )
    }

    fn render_directory_row(
        &self,
        position: usize,
        entry: &FileTreeEntry,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let palette = &self.theme().diff;
        let path = entry.path.clone();
        sidebar_row(entry.depth, self.sidebar_row_height())
            .id(format!("diff-directory:{path}"))
            .role(Role::TreeItem)
            .aria_label(entry.name.clone())
            .aria_level(entry.depth + 1)
            .aria_expanded(entry.expanded)
            .aria_selected(selected)
            .when(selected, |row| row.bg(color(palette.selection)))
            .text_color(color(palette.muted))
            .hover(|row| row.bg(color(palette.selection)))
            .on_click(cx.listener(move |viewer, _, window, cx| {
                viewer.toggle_directory(position, &path, window, cx);
            }))
            .child(
                div()
                    .w(px(DISCLOSURE_WIDTH))
                    .flex_shrink_0()
                    .text_center()
                    .child(if entry.expanded { "▾" } else { "▸" }),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(entry.name.clone()),
            )
            .child(self.stage_checkbox(entry, cx))
    }

    fn render_file_row(
        &self,
        index: usize,
        entry: &FileTreeEntry,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let palette = &self.theme().diff;
        let diff = self.document().files.get(index)?;
        let status_color = file_status_color(diff.status, palette);
        Some(
            sidebar_row(entry.depth, self.sidebar_row_height())
                .id(format!("diff-file:{}", diff.path))
                .role(Role::TreeItem)
                .aria_label(diff.path.to_string())
                .aria_level(entry.depth + 1)
                .aria_selected(selected)
                .when(selected, |row| row.bg(color(palette.selection)))
                .hover(|row| row.bg(color(palette.selection)))
                .on_click(cx.listener(move |viewer, _, window, cx| {
                    if viewer.handle_command(
                        DiffReviewCommand::Focus(ViewerPane::Files),
                        window,
                        cx,
                    ) {
                        viewer.handle_command(DiffReviewCommand::SelectFile(index), window, cx);
                    }
                }))
                .child(div().w(px(DISCLOSURE_WIDTH)).flex_shrink_0())
                .child(
                    div()
                        .w(px(14.0))
                        .flex_shrink_0()
                        .text_color(color(status_color))
                        .child(diff.status.code().to_string()),
                )
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(entry.name.clone()),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(self.metadata_font_size()))
                        .text_color(color(palette.muted))
                        .child(format!("+{} −{}", diff.additions(), diff.deletions())),
                )
                .child(self.stage_checkbox(entry, cx)),
        )
    }
}
