use crate::{ReviewHandle, ViewerPane, style};
use clankerdiff_core::{FilesMode, StageState};
use clankerdiff_rouge_ui::{
    components::{CheckState, Checkbox, MutedText, SelectionState},
    fonts::CODE_FONT,
    icon::IconName,
    resizable::{PanelSide, ResizablePanel},
    scrollbar::ListScrollbar,
    theme::{UiTheme, color},
    tokens::{ROW_HEIGHT, rem, scaled},
    tree::TreeRow,
    typography::{Typography, TypographyExt},
};
use rouge_core::{Align, RowKey, Styled, View, VirtualListHandle, div, dynamic, text, virtual_list};
use rouge_reactive::{effect, memo, signal};

pub(crate) fn sidebar(handle: ReviewHandle, list: VirtualListHandle, theme: UiTheme) -> impl View {
    let revision = handle.sidebar_revision();
    let status = handle.status();
    let mode = memo(move || status.get().files_mode);
    let count = memo({
        let handle = handle.clone();
        move || {
            revision.get();
            mode.get();
            handle.with(|state| state.tree().map_or(0, |tree| tree.entries().len()))
        }
    });
    let selection = handle.selection();
    let pane_selection = selection;
    let selected = memo(move || selection.get().sidebar_row);
    effect(move || {
        if let Some(row) = selected.get() {
            list.scroll_to_row(row, Align::Visible);
        }
    });
    let rows = virtual_list(move || count.get(), scaled(ROW_HEIGHT), {
        let handle = handle.clone();
        move |index| div().px(rem(4.0)).child(row(handle.clone(), index, theme))
    })
    .row_identity({
        move |index| {
            let revision = (revision.get(), mode.get());
            RowKey::new((index, revision), 0)
        }
    })
    .handle(list)
    .size_full();
    let width = signal(handle.sidebar_width_rem().get_untracked());
    effect({
        let preferred = handle.sidebar_width_rem();
        move || {
            width.set_if_changed(preferred.get());
        }
    });
    ResizablePanel::new("files-panel", PanelSide::Left, width)
        .min_width(0.0)
        .max_width(60.0)
        .on_resize({
            let handle = handle.clone();
            move |width| handle.resize_sidebar(Some(width))
        })
        .on_reset(move || handle.resize_sidebar(None))
        .child(
            div()
                .relative()
                .size_full()
                .flex()
                .col()
                .overflow_hidden()
                .bg(theme.paint(|c| c.surface))
                .border_r(rem(1.0))
                .border_color(
                    theme.paint(
                        move |c| {
                            if pane_selection.get().pane == ViewerPane::Files { c.accent } else { c.border }
                        },
                    ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h(0.0)
                        .relative()
                        .child(rows)
                        .child(div().absolute().right(0.0).top(0.0).bottom(0.0).child(ListScrollbar::list(list)))
                        .child(dynamic(move || {
                            div().children((count.get() == 0).then(|| {
                                div().p(rem(12.0)).child(MutedText::new(
                                    if mode.get() == FilesMode::Browse {
                                        "No repository files"
                                    } else {
                                        "No changed files"
                                    },
                                    theme,
                                ))
                            }))
                        })),
                ),
        )
}

fn row(handle: ReviewHandle, index: usize, theme: UiTheme) -> impl View {
    let Some(entry) = handle.with(|s| s.tree()?.entries().get(index).cloned()) else {
        return div().into_any();
    };
    let status = handle.status();
    let browsing = status.get_untracked().files_mode == FilesMode::Browse;
    let palette = handle.theme();
    let file_status =
        entry.file.and_then(|file| handle.with(|s| s.session().document().files.get(file).map(|f| f.status)));
    let selection = handle.selection();
    let id = entry.file.map_or_else(|| format!("directory-{}", entry.path), |file| format!("diff-file-{file}"));
    let directory = entry.directory;
    let mut row = TreeRow::new(id, entry.name.clone(), theme)
        .depth(entry.depth)
        .selection(move || {
            if selection.get().sidebar_row == Some(index) { SelectionState::Selected } else { SelectionState::None }
        })
        .on_click({
            let handle = handle.clone();
            move || handle.select_tree_entry(index, directory)
        });
    if directory {
        let accent = browsing && entry.changed;
        row = row
            .expanded(entry.expanded)
            .icon(if entry.expanded { IconName::FolderOpen } else { IconName::Folder })
            .icon_color(move || if accent { palette.with(|t| color(t.diff.accent)) } else { theme.colors().text_muted })
            .on_toggle({
                let handle = handle.clone();
                move || handle.select_tree_entry(index, true)
            });
    } else {
        row = row.icon(IconName::File).icon_color(move || {
            file_status.map_or_else(
                || theme.colors().text_muted,
                |status| palette.with(|t| color(style::file_status_color(status, &t.diff))),
            )
        });
        if !browsing && entry.changed {
            let stat = entry.stat;
            row = row.child(
                div()
                    .flex()
                    .flex_none()
                    .gap(rem(4.0))
                    .child(
                        text(format!("+{}", stat.added))
                            .typography(Typography::Caption)
                            .font(CODE_FONT)
                            .color(move || palette.with(|t| color(t.diff.addition))),
                    )
                    .child(
                        text(format!("−{}", stat.deleted))
                            .typography(Typography::Caption)
                            .font(CODE_FONT)
                            .color(move || palette.with(|t| color(t.diff.deletion))),
                    ),
            );
        }
        if let Some(file_status) = file_status {
            row = row.child(
                div().w(rem(12.0)).flex_none().flex().justify_center().child(
                    text(file_status.code().to_string())
                        .typography(Typography::Caption)
                        .font(CODE_FONT)
                        .color(move || palette.with(|t| color(style::file_status_color(file_status, &t.diff)))),
                ),
            );
        }
    }
    if !browsing && (directory || entry.changed) {
        let path = entry.path;
        let state = match entry.stage {
            StageState::Staged => CheckState::Checked,
            StageState::PartiallyStaged => CheckState::Indeterminate,
            StageState::Unstaged => CheckState::Unchecked,
        };
        let checkbox_id = entry
            .file
            .map_or_else(|| format!("diff-directory-checkbox:{path}"), |file| format!("diff-file-checkbox:{file}"));
        row = row.child(dynamic(move || {
            let status = status.get();
            if !status.context.capabilities.repository || status.files_mode != FilesMode::Diff {
                return div().into_any();
            }
            Checkbox::new(checkbox_id.clone(), state, theme)
                .tooltip(format!("Toggle staging for {path}"))
                .disabled(status.context.repository_pending)
                .on_toggle({
                    let handle = handle.clone();
                    let path = path.clone();
                    move || handle.stage_tree_entry(&path)
                })
                .into_any()
        }));
    }
    row.into_any()
}
