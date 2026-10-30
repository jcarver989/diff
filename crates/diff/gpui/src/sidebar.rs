use crate::{
    DiffViewer, ViewerPane,
    style::{color, file_status_color},
    ui::{
        prelude::{
            CODE_FONT, IconButton, IconName, PanelSide, ResizablePanel, TreeRow, Typography, TypographyExt, rem,
        },
        tokens::CHECKBOX_SIZE,
    },
    viewer::{MAX_SIDEBAR_WIDTH, MIN_SIDEBAR_WIDTH},
};
use clankerdiff_core::{DiffReviewCommand, FileTreeEntry, FilesMode, StageState};
use gpui::{AnyElement, Context, Div, PathBuilder, Role, canvas, div, point, prelude::*, px, uniform_list};
use gpui_base::{Checkbox, CheckboxIndicator, CheckboxState};
use std::ops::Range;

impl DiffViewer {
    pub(crate) fn render_sidebar_toggle(&self, cx: &mut Context<Self>) -> IconButton {
        let open = !self.sidebar_collapsed();
        let label = match self.files_mode() {
            FilesMode::Browse => "files",
            FilesMode::Diff => "changed files",
        };
        PanelSide::Left.toggle_button("diff-sidebar-toggle", open, label, self.ui_theme()).on_click(cx.listener(
            move |viewer, _, _, cx| {
                viewer.set_sidebar_collapsed(open, cx);
            },
        ))
    }

    pub(crate) fn reveal_focused_sidebar(&mut self, cx: &mut Context<Self>) {
        if self.pane() == ViewerPane::Files {
            self.set_sidebar_collapsed(false, cx);
        }
    }

    fn stage_checkbox(&self, entry: &FileTreeEntry, cx: &mut Context<Self>) -> Checkbox {
        let colors = self.ui_theme().colors;
        let checkbox_state = match entry.stage {
            StageState::Staged => CheckboxState::Checked,
            StageState::PartiallyStaged => CheckboxState::Indeterminate,
            StageState::Unstaged => CheckboxState::Unchecked,
        };
        let viewer = cx.entity().downgrade();
        let path = entry.path.clone();
        let label = format!("Toggle staging for {path}");
        let mark_color = colors.text;
        let (checkbox_id, id) = if let Some(index) = entry.file {
            (format!("diff-file-checkbox:{path}"), format!("diff-file-checkbox:{index}"))
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
                CheckboxIndicator::new().state(checkbox_state).size_full().child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, (), window, _| {
                            if checkbox_state == CheckboxState::Unchecked {
                                return;
                            }
                            let mut path = PathBuilder::stroke(px(2.0));
                            let at = |x: f32, y: f32| {
                                point(bounds.left() + bounds.size.width * x, bounds.top() + bounds.size.height * y)
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
        let selector = match self.files_mode() {
            FilesMode::Browse => "browse-files",
            FilesMode::Diff => "diff-files",
        };
        let rows = div()
            .id(selector)
            .debug_selector(move || selector.to_owned())
            .role(Role::Tree)
            .flex_1()
            .min_h_0()
            .p_1()
            .child(match self.tree() {
                Some(tree) => uniform_list(
                    "diff-file-rows",
                    tree.entries().len(),
                    cx.processor(|viewer, range: Range<usize>, _window, cx| viewer.render_sidebar_rows(range, cx)),
                )
                .size_full()
                .track_scroll(&self.sidebar_scroll_handle)
                .into_any_element(),
                None => self.render_listing_status().into_any_element(),
            });

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
            .when(self.pane == ViewerPane::Files, |panel| panel.border_color(colors.accent))
            .child(rows)
    }

    fn render_listing_status(&self) -> Div {
        div()
            .debug_selector(|| "browse-files-status".to_owned())
            .p_2()
            .typography(Typography::Caption)
            .text_color(self.ui_theme().colors.text_muted)
            .child("Loading files…")
    }

    fn render_sidebar_rows(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        self.sidebar_rendered_rows = range.clone();
        let Some(tree) = self.tree() else {
            return Vec::new();
        };
        let selected = tree.selected_position();
        let entries = tree.entries();
        range
            .filter_map(|position| {
                let entry = entries.get(position)?;
                let selected = selected == Some(position);
                Some(if entry.directory {
                    self.render_directory_row(position, entry, selected, cx).into_any_element()
                } else {
                    self.render_file_row(entry, selected, cx).into_any_element()
                })
            })
            .collect()
    }

    fn render_directory_row(
        &self,
        position: usize,
        entry: &FileTreeEntry,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> TreeRow {
        let path = entry.path.clone();
        let folder = if entry.expanded { IconName::FolderOpen } else { IconName::Folder };
        let browsing = self.files_mode() == FilesMode::Browse;
        let selector = if browsing { "browse-directory" } else { "diff-directory" };
        let changed = (browsing && entry.changed).then(|| color(self.theme().diff.accent));
        TreeRow::new(format!("{selector}:{path}"), entry.name.clone(), self.ui_theme())
            .depth(entry.depth)
            .expanded(entry.expanded)
            .icon(folder, changed)
            .selected(selected)
            .on_click(cx.listener(move |viewer, _, window, cx| {
                viewer.toggle_directory(position, &path, window, cx);
            }))
            .when(!browsing, |row| row.child(self.stage_checkbox(entry, cx)))
    }

    fn render_file_row(&self, entry: &FileTreeEntry, selected: bool, cx: &mut Context<Self>) -> TreeRow {
        let palette = &self.theme().diff;
        let browsing = self.files_mode() == FilesMode::Browse;
        let diff = entry.file.and_then(|index| self.document().files.get(index));
        let status_color = diff.map(|diff| color(file_status_color(diff.status, palette)));
        let selector = if browsing { "browse-file" } else { "diff-file" };
        let (path, file) = (entry.path.clone(), entry.file);
        TreeRow::new(format!("{selector}:{path}"), entry.name.clone(), self.ui_theme())
            .aria_label(entry.path.clone())
            .depth(entry.depth)
            .icon(IconName::File, status_color)
            .selected(selected)
            .on_click(cx.listener(move |viewer, _, window, cx| {
                if !viewer.handle_command(DiffReviewCommand::Focus(ViewerPane::Files), window, cx) {
                    return;
                }
                match file {
                    Some(index) => {
                        viewer.handle_command(DiffReviewCommand::SelectFile(index), window, cx);
                    }
                    None => viewer.browse_file(&path, window, cx),
                }
            }))
            .when(!browsing, |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .gap_1()
                        .typography(Typography::Caption)
                        .font_family(CODE_FONT)
                        .child(div().text_color(color(palette.addition)).child(format!("+{}", entry.stat.added)))
                        .child(div().text_color(color(palette.deletion)).child(format!("−{}", entry.stat.deleted))),
                )
            })
            .children(diff.map(|diff| {
                div()
                    .w(rem(12.0))
                    .flex_shrink_0()
                    .text_center()
                    .typography(Typography::Caption)
                    .font_family(CODE_FONT)
                    .text_color(status_color.unwrap_or_default())
                    .child(diff.status.code().to_string())
            }))
            .when(!browsing, |row| row.child(self.stage_checkbox(entry, cx)))
    }
}
