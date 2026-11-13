use super::MarkdownHandle;
use clankerdiff_core::ReviewCommand;
use clankerdiff_markdown::{MarkdownFocusPane, MarkdownReviewCommand};
use rouge_core::{RowKey, Styled, Theme, View, component, div, dynamic, px, text, view, virtual_list};
use rouge_widgets::tokens::ROW_HEIGHT;
use rouge_widgets::{
    ButtonVariant, CODE_FONT, ControlSize, IconName, ModalSize, SelectionState, Side, UiStyles, button, icon_button,
    modal, panel_toggle, resizable_panel, tree_row, vertical_scrollbar,
};
use std::path::Path;

const HEADER_HEIGHT: f32 = 56.0;
const MIN_OUTLINE_WIDTH: f32 = 160.0 / 16.0;
const MAX_OUTLINE_WIDTH: f32 = 420.0 / 16.0;

type Shortcuts = &'static [(&'static str, &'static str)];

const NAVIGATION: Shortcuts = &[
    ("j / k, ↑ / ↓", "move selection"),
    ("n / p", "next / previous heading"),
    ("g / G, Home / End", "first / last block"),
    ("PgUp / PgDn", "move a page"),
    ("Tab", "switch document / outline"),
    ("Enter", "open the selected heading"),
    ("b", "hide / show outline"),
];
const REVIEW: Shortcuts = &[
    ("c / e / x", "add / edit / delete comment"),
    ("u", "undo last comment"),
    ("a", "approve"),
    ("r", "send feedback"),
    ("y", "copy review"),
    ("t", "choose theme"),
    ("Esc", "cancel review"),
];
const DRAFT: Shortcuts = &[("Enter", "save comment"), ("Shift-Enter", "insert newline"), ("Esc", "cancel comment")];

#[component]
pub(super) fn header(handle: &MarkdownHandle) -> impl View {
    let handle = *handle;
    let leading = move || {
        let toggle = handle.outline_available().then(|| {
            let open = !handle.outline_collapsed.get();
            view! {
                <panel_toggle side=Side::Left id="markdown-outline-toggle" open name="outline"
                              on_click={move || handle.toggle_outline()}/>
            }
        });
        view! {
            <div flex_1 min_w=0.0 flex row items_center gap=8.0>
                {toggle}
                <title handle={&handle}/>
            </div>
        }
    };
    let trailing = move || {
        handle.overlay.get();
        handle.draft.get();
        handle.annotations.get();
        let caps = handle.capabilities.get();
        let count = handle.with(|session| session.review().len());
        let (label, command) = if count == 0 {
            ("Approve".to_owned(), MarkdownReviewCommand::Approve)
        } else {
            (format!("Send feedback ({count})"), MarkdownReviewCommand::RequestChanges)
        };
        let run = |command: MarkdownReviewCommand| move || handle.run(command);
        let copy = (caps.clipboard && count > 0).then(|| {
            view! {
                <icon_button id="markdown-copy" icon=IconName::Copy label="Copy review"
                             on_click={move || handle.run(MarkdownReviewCommand::CopyReview(handle.copy_decision()))}/>
            }
        });
        view! {
            <div flex flex_none row items_center gap=8.0>
                <icon_button id="markdown-shortcuts" icon=IconName::Info label="Keyboard shortcuts"
                             on_click=run(ReviewCommand::ShowHelp.into())/>
                <button id="markdown-theme" label="Theme" size=ControlSize::Small
                        disabled={!handle.command_enabled(ReviewCommand::OpenThemePicker.into())}
                        on_click=run(ReviewCommand::OpenThemePicker.into())/>
                {copy}
                <button id="markdown-submit" label variant=ButtonVariant::Primary size=ControlSize::Small
                        disabled={!handle.command_enabled(command)} on_click=run(command)/>
            </div>
        }
    };
    view! {
        <div test_id="markdown-header" h=HEADER_HEIGHT flex_none flex row items_center gap=8.0 px=12.0
             border_b=px(1.0) border_color=Theme::current().border>
            <dynamic children=leading/>
            <dynamic children=trailing/>
        </div>
    }
}

#[component]
fn title(handle: &MarkdownHandle) -> impl View {
    let (title, path) = handle.rows.with(|rows| {
        let document = &rows.document;
        let path = document.source_path().map(str::to_owned);
        let title = document
            .title()
            .map(str::to_owned)
            .or_else(|| {
                path.as_deref()
                    .and_then(|path| Path::new(path).file_name())
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "Review".to_owned());
        (title, path)
    });
    let path = path.map(|path| {
        view! {
            <div test_id="markdown-path"><text caption font=CODE_FONT no_wrap muted>{path}</text></div>
        }
    });
    view! {
        <div flex_1 min_w=0.0 px=4.0 flex col overflow_hidden>
            <div test_id="markdown-title"><text title no_wrap primary>{title}</text></div>
            {path}
        </div>
    }
}

pub(super) fn outline(handle: &MarkdownHandle) -> impl View + use<> {
    let (rows, revision, pane, list, width) =
        (handle.rows, handle.revision, handle.pane, handle.outline, handle.outline_width);
    let (handle, theme) = (*handle, Theme::current());
    view! {
        <resizable_panel test_id="markdown-outline" edge=Side::Left size=width min_size=MIN_OUTLINE_WIDTH
                         max_size=MAX_OUTLINE_WIDTH on_resize={move |value: f32| width.set(value)}
                         on_reset={move || width.set(super::DEFAULT_OUTLINE_WIDTH)}>
            <div size_full flex col overflow_hidden bg=theme.surface border_r=px(1.0)
                 border_color={move || {
                     if pane.get() == MarkdownFocusPane::Outline { theme.accent.get() } else { theme.border.get() }
                 }}>
                <div px=12.0 pt=16.0 pb=8.0><text label muted>"Contents"</text></div>
                <div flex_1 min_h=0.0 relative pb=8.0>
                    <virtual_list count={move || rows.with(|rows| rows.document.outline().len())}
                                  row_height=ROW_HEIGHT
                                  row_identity={move |index| RowKey::new(index, revision.get())}
                                  handle=list size_full let:index>
                        <div px=4.0><outline_row handle={&handle} index/></div>
                    </virtual_list>
                    <div absolute right=0.0 top=0.0 bottom=0.0>{vertical_scrollbar(list.scroll_state())}</div>
                </div>
            </div>
        </resizable_panel>
    }
}

#[component]
fn outline_row(handle: &MarkdownHandle, index: usize) -> Option<impl View> {
    let heading = handle.with(|session| session.document().outline().get(index).cloned())?;
    let (handle, theme) = (*handle, Theme::current());
    let (selected, annotations) = (handle.selected, handle.annotations);
    let target = heading.target_id;
    Some(view! {
        <tree_row id={format!("markdown-heading-{index}")} label=heading.title
                  depth={usize::from(heading.level.saturating_sub(1))}
                  label_color={if heading.level <= 1 { theme.text } else { theme.text_secondary }}
                  selection={move || {
                      selected.get();
                      if handle.active_heading() == Some(index) {
                          SelectionState::Selected
                      } else {
                          SelectionState::None
                      }
                  }}
                  on_click={move || handle.select(target, MarkdownFocusPane::Outline)}>
            <dynamic>{
                annotations.get();
                let count = handle.section_comment_count(index);
                (count > 0).then(|| view! {
                    <div flex_none px=6.0 rounded=ROW_HEIGHT bg=theme.surface_selected>
                        <text caption accent>{count.to_string()}</text>
                    </div>
                })
            }</dynamic>
        </tree_row>
    })
}

impl MarkdownHandle {
    fn section_comment_count(&self, index: usize) -> usize {
        self.with(|session| {
            let document = session.document();
            let outline = document.outline();
            let Some(heading) = outline.get(index) else {
                return 0;
            };
            let end = outline.get(index + 1).map_or(usize::MAX, |next| next.target_id.index());
            let section = heading.target_id.index()..end;
            session
                .review()
                .comments()
                .iter()
                .filter_map(|comment| document.exact_target(&comment.anchor))
                .filter(|target| section.contains(&target.index()))
                .count()
        })
    }
}

pub(super) fn shortcuts(close: impl Fn() + 'static) -> impl View {
    let section = |title: &'static str, rows: Shortcuts| {
        view! {
            <div flex_1 flex col gap=4.0>
                <div mb=4.0><text label accent>{title}</text></div>
                {..rows.iter().map(move |(keys, description)| {
                    view! {
                        <div flex row gap=12.0>
                            <div w=136.0 flex_none><text code no_wrap primary>{*keys}</text></div>
                            <div flex_1 min_w=0.0><text body muted>{*description}</text></div>
                        </div>
                    }
                })}
            </div>
        }
    };
    let sections = [("Navigation", NAVIGATION), ("Review", REVIEW), ("Comment editor", DRAFT)];
    let rows = sections.chunks(2).enumerate().map(|(index, pair)| {
        view! {
            <div flex row gap=20.0 when:mt=(index > 0, 16.0)>
                {..pair.iter().map(|(title, rows)| section(title, rows))}
            </div>
        }
    });
    view! {
        <modal id="markdown-shortcuts-help" title="Review shortcuts" hint="Esc to close" size=ModalSize::Wide
               on_dismiss=close>
            {..rows.collect::<Vec<_>>()}
        </modal>
    }
}
