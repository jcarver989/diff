use super::MarkdownHandle;
use clankerdiff_core::ReviewCommand;
use clankerdiff_markdown::{MarkdownAnchors, MarkdownFocusPane, MarkdownReviewCommand};
use rouge_core::{Role, RowKey, Styled, Theme, View, component, div, dynamic, text, view, virtual_list};
use rouge_widgets::tokens::{HAIRLINE, ROW_HEIGHT, Radius, Space, UNIT};
use rouge_widgets::{
    ButtonVariant, CODE_FONT, ControlSize, IconName, ModalSize, SelectionState, Side, UiStyles, button, icon_button,
    kbd, modal, resizable_panel, tree_row, vertical_scrollbar,
};
use std::path::Path;

const HEADER_HEIGHT: f32 = 14.0 * UNIT;
const SHORTCUT_KEYS_WIDTH: f32 = 34.0 * UNIT;
const MIN_OUTLINE_WIDTH: f32 = 160.0;
const MAX_OUTLINE_WIDTH: f32 = 420.0;

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
            view! {
                <icon_button test_id="markdown-outline-toggle" icon=IconName::PanelLeft label="Toggle outline"
                             selected={move || !handle.outline_collapsed.get()}
                             on_click={move || handle.toggle_outline()}/>
            }
        });
        view! {
            <div flex_1 min_w=0.0 flex row items_center gap=Space::X2>
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
                <icon_button test_id="markdown-copy" icon=IconName::Copy label="Copy review"
                             on_click={move || handle.run(MarkdownReviewCommand::CopyReview(handle.copy_decision()))}/>
            }
        });
        view! {
            <div flex flex_none row items_center gap=Space::X2>
                <icon_button test_id="markdown-shortcuts" icon=IconName::Info label="Keyboard shortcuts"
                             on_click=run(ReviewCommand::ShowHelp.into())/>
                <button test_id="markdown-theme" label="Theme" control_size=ControlSize::Small
                        disabled={!handle.command_enabled(ReviewCommand::OpenThemePicker.into())}
                        on_click=run(ReviewCommand::OpenThemePicker.into())/>
                {copy}
                <button test_id="markdown-submit" label variant=ButtonVariant::Primary control_size=ControlSize::Small
                        disabled={!handle.command_enabled(command)} on_click=run(command)/>
            </div>
        }
    };
    view! {
        <div test_id="markdown-header" h=HEADER_HEIGHT flex_none flex row items_center gap=Space::X2 px=Space::X3
             border_b=HAIRLINE border_color=Role::Border>
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
        <div flex_1 min_w=0.0 px=Space::X1 flex col overflow_hidden>
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
                         on_reset={move || width.set(super::DEFAULT_OUTLINE_WIDTH)}
                         divider_color={move || if pane.get() == MarkdownFocusPane::Outline { theme.get().accent } else { theme.get().border }}>
            <div size_full flex col overflow_hidden bg=Role::Surface>
                <div px=Space::X3 pt=Space::X4 pb=Space::X2><text label muted>"Contents"</text></div>
                <div flex_1 min_h=0.0 relative pb=Space::X2>
                    <virtual_list count={move || rows.with(|rows| rows.headings)}
                                  row_height=ROW_HEIGHT
                                  row_identity={move |index| RowKey::new(index, revision.get())}
                                  handle=list size_full let:index>
                        <div px=Space::X1><outline_row handle={&handle} index/></div>
                    </virtual_list>
                    <div absolute right=0.0 top=0.0 bottom=0.0>{vertical_scrollbar(list.scroll_state())}</div>
                </div>
            </div>
        </resizable_panel>
    }
}

#[component]
fn outline_row(handle: &MarkdownHandle, index: usize) -> Option<impl View> {
    let heading = handle.with(|session| session.outline().get(index).cloned())?;
    let handle = *handle;
    let (selected, annotations) = (handle.selected, handle.annotations);
    let target = heading.target_id;
    Some(view! {
        <tree_row test_id={format!("markdown-heading-{index}")} label=heading.title
                  depth={usize::from(heading.level.saturating_sub(1))}
                  label_color={if heading.level <= 1 { Role::Text } else { Role::TextSecondary }}
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
                    <div flex_none px=Space::X1 rounded=Radius::Inner bg=Role::SurfaceSelected>
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
            let outline = session.outline();
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
            <div flex_1 flex col gap=Space::X1>
                <div mb=Space::X1><text label accent>{title}</text></div>
                {..rows.iter().map(move |(keys, description)| {
                    view! {
                        <div flex row gap=Space::X3>
                            <div w=SHORTCUT_KEYS_WIDTH flex_none flex row flex_wrap items_center gap=Space::X1>
                                {..keys.split([',', '/']).map(|key| kbd(key.trim().to_owned()))}
                            </div>
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
            <div flex row gap=Space::X6 when:mt=(index > 0, Space::X4)>
                {..pair.iter().map(|(title, rows)| section(title, rows))}
            </div>
        }
    });
    view! {
        <modal test_id="markdown-shortcuts-help" title="Review shortcuts" hint="Esc to close" modal_size=ModalSize::Wide
               on_dismiss=close>
            {..rows.collect::<Vec<_>>()}
        </modal>
    }
}
