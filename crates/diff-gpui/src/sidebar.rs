use crate::{
    DiffViewer, ViewerPane,
    style::{color, file_status_color},
    ui::{
        prelude::{
            CODE_FONT, IconName, PanelSide, ResizablePanel, TreeRow, Typography, TypographyExt, rem,
        },
        tokens::{BAR_HEIGHT, CHECKBOX_SIZE},
    },
    viewer::{MAX_SIDEBAR_WIDTH, MIN_SIDEBAR_WIDTH},
};
use clankerdiff_core::{DiffReviewCommand, FileTree, FileTreeEntry, StageState};
use gpui::{Context, PathBuilder, Role, canvas, div, point, prelude::*, px};
use gpui_base::{Checkbox, CheckboxIndicator, CheckboxState};

impl DiffViewer {
    fn stage_checkbox(&self, entry: &FileTreeEntry, cx: &mut Context<Self>) -> Checkbox {
        let colors = self.ui_theme().colors;
        let checkbox_state = match FileTree::stage_state_for(self.document(), &entry.path) {
            StageState::Staged => CheckboxState::Checked,
            StageState::PartiallyStaged => CheckboxState::Indeterminate,
            StageState::Unstaged => CheckboxState::Unchecked,
        };
        let viewer = cx.entity().downgrade();
        let path = entry.path.clone();
        let label = format!("Toggle staging for {path}");
        let mark_color = colors.text;
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
            .size(rem(CHECKBOX_SIZE))
            .flex_shrink_0()
            .cursor_pointer()
            .border_1()
            .border_color(colors.text_muted)
            .bg(colors.canvas)
            .hover(|style| style.border_color(colors.accent))
            .focus_visible(|style| style.border_color(colors.accent))
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

    pub(crate) fn render_sidebar(&self, cx: &mut Context<Self>) -> ResizablePanel {
        let theme = self.ui_theme();
        let colors = theme.colors;
        let mut rows = div()
            .id("diff-files")
            .debug_selector(|| "diff-files".to_owned())
            .role(Role::Tree)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.sidebar_scroll_handle)
            .p_1()
            .flex()
            .flex_col();

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

        let viewer = cx.entity().downgrade();
        let reset = viewer.clone();
        ResizablePanel::new("diff-sidebar", PanelSide::Left, self.sidebar_width(), theme)
            .min_width(MIN_SIDEBAR_WIDTH)
            .max_width(MAX_SIDEBAR_WIDTH)
            .on_resize(move |width, window, cx| {
                let _ = viewer.update(cx, |viewer, cx| viewer.resize_sidebar(width, window, cx));
            })
            .on_reset(move |_, cx| {
                let _ = reset.update(cx, DiffViewer::reset_sidebar_width);
            })
            .bg(colors.surface)
            .when(self.pane == ViewerPane::Files, |panel| {
                panel.border_color(colors.accent)
            })
            .child(
                div()
                    .h(rem(BAR_HEIGHT))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .border_b_1()
                    .border_color(colors.border)
                    .child(
                        div()
                            .typography(Typography::Label)
                            .text_color(colors.text_secondary)
                            .child("Changed files"),
                    )
                    .child(
                        div()
                            .px_1p5()
                            .rounded_full()
                            .bg(colors.surface_selected)
                            .typography(Typography::Caption)
                            .text_color(colors.text_muted)
                            .child(self.document().files.len().to_string()),
                    ),
            )
            .child(rows)
    }

    fn render_directory_row(
        &self,
        position: usize,
        entry: &FileTreeEntry,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> TreeRow {
        let path = entry.path.clone();
        let folder = if entry.expanded {
            IconName::FolderOpen
        } else {
            IconName::Folder
        };
        TreeRow::new(
            format!("diff-directory:{path}"),
            entry.name.clone(),
            self.ui_theme(),
        )
        .depth(entry.depth)
        .expanded(entry.expanded)
        .icon(folder, None)
        .selected(selected)
        .on_click(cx.listener(move |viewer, _, window, cx| {
            viewer.toggle_directory(position, &path, window, cx);
        }))
        .child(self.stage_checkbox(entry, cx))
    }

    fn render_file_row(
        &self,
        index: usize,
        entry: &FileTreeEntry,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> Option<TreeRow> {
        let palette = &self.theme().diff;
        let diff = self.document().files.get(index)?;
        let status_color = color(file_status_color(diff.status, palette));
        Some(
            TreeRow::new(
                format!("diff-file:{}", diff.path),
                entry.name.clone(),
                self.ui_theme(),
            )
            .aria_label(diff.path.to_string())
            .depth(entry.depth)
            .icon(IconName::File, Some(status_color))
            .selected(selected)
            .on_click(cx.listener(move |viewer, _, window, cx| {
                if viewer.handle_command(DiffReviewCommand::Focus(ViewerPane::Files), window, cx) {
                    viewer.handle_command(DiffReviewCommand::SelectFile(index), window, cx);
                }
            }))
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .gap_1()
                    .typography(Typography::Caption)
                    .font_family(CODE_FONT)
                    .child(
                        div()
                            .text_color(color(palette.addition))
                            .child(format!("+{}", diff.additions())),
                    )
                    .child(
                        div()
                            .text_color(color(palette.deletion))
                            .child(format!("−{}", diff.deletions())),
                    ),
            )
            .child(
                div()
                    .w(rem(12.0))
                    .flex_shrink_0()
                    .text_center()
                    .typography(Typography::Caption)
                    .font_family(CODE_FONT)
                    .text_color(status_color)
                    .child(diff.status.code().to_string()),
            )
            .child(self.stage_checkbox(entry, cx)),
        )
    }
}
