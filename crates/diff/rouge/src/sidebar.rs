use crate::{ReviewHandle, ViewerPane, style};
use clankerdiff_core::{FilesMode, StageState};
use clankerdiff_rouge_ui::{
    components::{CheckState, SelectionState, checkbox, muted_text},
    fonts::CODE_FONT,
    icon::IconName,
    resizable::{PanelSide, resizable_panel},
    scrollbar::ListScrollbar,
    theme::{UiTheme, color},
    tokens::ROW_HEIGHT,
    tree::tree_row,
    typography::UiStyles,
};
use rouge_core::{Align, Div, RowKey, Styled, View, VirtualListHandle, div, dynamic, text, view, virtual_list};
use rouge_reactive::{effect, memo, signal};

pub(crate) fn sidebar(handle: ReviewHandle, list: VirtualListHandle) -> impl View {
    let theme = UiTheme::current();
    let revision = handle.sidebar_revision();
    let status = handle.status();
    let mode = memo(move || status.get().files_mode);
    let count = memo(move || {
        revision.get();
        mode.get();
        handle.with(|state| state.tree().map_or(0, |tree| tree.entries().len()))
    });
    let selection = handle.selection();
    let selected = memo(move || selection.get().sidebar_row);
    effect(move || {
        if let Some(row) = selected.get() {
            list.scroll_to_row(row, Align::Visible);
        }
    });
    let preferred = handle.sidebar_width_rem();
    let width = signal(preferred.get_untracked());
    effect(move || {
        width.set_if_changed(preferred.get());
    });
    view! {
        <resizable_panel id="files-panel" side=PanelSide::Left width_in_rem=width min_width=0.0 max_width=60.0
                         on_resize={move |width: f32| handle.resize_sidebar(Some(width))}
                         on_reset={move || handle.resize_sidebar(None)}>
            <div relative size_full flex col overflow_hidden bg=theme.surface border_r=1.0
                 border_color=theme.paint(move |c| {
                     if selection.get().pane == ViewerPane::Files { c.accent } else { c.border }
                 })>
                <div flex_1 min_h=0.0 relative>
                    <virtual_list count={move || count.get()} row_height=ROW_HEIGHT
                                  row_identity={move |index| RowKey::new((index, (revision.get(), mode.get())), 0)}
                                  handle=list size_full let:index>
                        <div px=4.0>{row(handle, index)}</div>
                    </virtual_list>
                    <div absolute right=0.0 top=0.0 bottom=0.0>{ListScrollbar::list(list)}</div>
                    <dynamic>{
                        let empty = if mode.get() == FilesMode::Browse { "No repository files" } else { "No changed files" };
                        view! {
                            <div>{(count.get() == 0).then(|| view! { <div p=12.0><muted_text>{empty}</muted_text></div> })}</div>
                        }
                    }</dynamic>
                </div>
            </div>
        </resizable_panel>
    }
}

fn row(handle: ReviewHandle, index: usize) -> Option<Div> {
    let theme = UiTheme::current();
    let entry = handle.with(|s| s.tree()?.entries().get(index).cloned())?;
    let status = handle.status();
    let browsing = status.get_untracked().files_mode == FilesMode::Browse;
    let palette = handle.theme();
    let file_status =
        entry.file.and_then(|file| handle.with(|s| s.session().document().files.get(file).map(|f| f.status)));
    let selection = handle.selection();
    let id = entry.file.map_or_else(|| format!("directory-{}", entry.path), |file| format!("diff-file-{file}"));
    let directory = entry.directory;
    let accent = browsing && entry.changed;
    let icon = match (directory, entry.expanded) {
        (true, true) => IconName::FolderOpen,
        (true, false) => IconName::Folder,
        (false, _) => IconName::File,
    };
    let icon_color = move || {
        let status_color = |status| palette.with(|t| color(style::file_status_color(status, &t.diff)));
        match (directory, file_status) {
            (true, _) if accent => palette.with(|t| color(t.diff.accent)),
            (false, Some(status)) => status_color(status),
            _ => theme.text_muted.get(),
        }
    };
    let stat = (!directory && !browsing && entry.changed).then(|| {
        let stat = entry.stat;
        view! {
            <div flex flex_none gap=4.0>
                <text caption font=CODE_FONT color={move || palette.with(|t| color(t.diff.addition))}>
                    {format!("+{}", stat.added)}
                </text>
                <text caption font=CODE_FONT color={move || palette.with(|t| color(t.diff.deletion))}>
                    {format!("−{}", stat.deleted)}
                </text>
            </div>
        }
    });
    let code = file_status.filter(|_| !directory).map(|file_status| {
        view! {
            <div w=12.0 flex_none flex justify_center>
                <text caption font=CODE_FONT
                      color={move || palette.with(|t| color(style::file_status_color(file_status, &t.diff)))}>
                    {file_status.code().to_string()}
                </text>
            </div>
        }
    });
    let staging = (!browsing && (directory || entry.changed)).then(|| {
        let path = entry.path.clone();
        let state = match entry.stage {
            StageState::Staged => CheckState::Checked,
            StageState::PartiallyStaged => CheckState::Indeterminate,
            StageState::Unstaged => CheckState::Unchecked,
        };
        let checkbox_id = entry
            .file
            .map_or_else(|| format!("diff-directory-checkbox:{path}"), |file| format!("diff-file-checkbox:{file}"));
        dynamic(move || {
            let status = status.get();
            if !status.context.capabilities.repository || status.files_mode != FilesMode::Diff {
                return None;
            }
            let path = path.clone();
            Some(view! {
                <checkbox id={checkbox_id.clone()} state tooltip={format!("Toggle staging for {path}")}
                          disabled=status.context.repository_pending on_toggle={move || handle.stage_tree_entry(&path)}/>
            })
        })
    });
    Some(view! {
        <tree_row id label={entry.name.clone()} depth=entry.depth expanded?={directory.then_some(entry.expanded)} icon
                  icon_color
                  selection={move || {
                      if selection.get().sidebar_row == Some(index) {
                          SelectionState::Selected
                      } else {
                          SelectionState::None
                      }
                  }}
                  on_click={move || handle.select_tree_entry(index, directory)}
                  on_toggle={move || handle.select_tree_entry(index, true)}>
            {stat}
            {code}
            {staging}
        </tree_row>
    })
}
