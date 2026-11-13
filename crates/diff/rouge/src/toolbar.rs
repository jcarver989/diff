use crate::{ReviewHandle, ViewerPane, shortcut_label, style};
use clankerdiff_core::{
    ControlKind, DiffReviewCommand as C, FilesMode, PageLabel, ReviewCommand, Toolbar, ToolbarControl, ViewMode,
    tooltip,
};
use rouge_core::{
    AnyView, Color, Cursor, Div, Length, Rect, Styled, Theme, View, div, dynamic, px, root_font_size, text, view,
};
use rouge_reactive::{memo, signal, use_context};
use rouge_widgets::{
    ButtonVariant, CODE_FONT, ControlSize, Icon, IconName, InterfaceSize, NoticeTone, SizeAction, UiStyles, button,
    icon_button, muted_text, notification, segment, segmented_control, tokens,
};
use rouge_widgets::{Hint, popover};

const COMPACT_TOOLBAR_WIDTH: f32 = 900.0;
const MIN_SPLIT_TOOLBAR_WIDTH: f32 = 640.0;
const MAX_STEPPER_WIDTH: f32 = 360.0;
const MIN_STEPPER_WIDTH: f32 = 160.0;

#[derive(Clone, PartialEq)]
struct Model {
    toolbar: Toolbar,
    enabled: Vec<(C, bool)>,
    files_mode: FilesMode,
    changes: usize,
    split: bool,
    compact: bool,
    wrap: bool,
    flags: Flags,
    error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Flags {
    browsable: bool,
    files_focused: bool,
    pending: bool,
}

impl Model {
    fn enabled(&self, command: &C) -> bool {
        self.enabled.iter().find(|(known, _)| known == command).is_some_and(|(_, enabled)| *enabled)
    }
}

pub(crate) fn toolbar(handle: &ReviewHandle) -> impl View {
    let theme = Theme::current();
    let handle = *handle;
    let status = handle.status();
    let revision = handle.presentation_revision();
    let selection = handle.selection();
    let sidebar = handle.sidebar_width_rem();
    let bounds = signal(Rect::ZERO);
    let font = root_font_size();
    let model = memo({
        move || {
            let status = status.get();
            revision.get();
            let pane = selection.get().pane;
            let width = bounds.get().size.width / font.get();
            let rems = |design: f32| design / tokens::BASE_SIZE;
            let beside = width - sidebar.get();
            let split = !status.sidebar_collapsed && beside >= rems(MIN_SPLIT_TOOLBAR_WIDTH);
            let room = if split { beside } else { width };
            handle.with(|state| {
                let session = state.session();
                let toolbar =
                    Toolbar::new(session, Some(status.sidebar_collapsed), status.context.browsable, shortcut_label);
                let enabled = [&toolbar.files, &toolbar.history, &toolbar.stepper, &toolbar.scopes, &toolbar.layouts]
                    .into_iter()
                    .flatten()
                    .map(|control| control.command.clone())
                    .chain([C::SetFilesMode(FilesMode::Diff), C::SetFilesMode(FilesMode::Browse), C::ToggleWrap])
                    .map(|command| {
                        let enabled = state.command_enabled(&command);
                        (command, enabled)
                    })
                    .collect();
                Model {
                    toolbar,
                    enabled,
                    files_mode: status.files_mode,
                    changes: session.document().files.len(),
                    split,
                    compact: room < rems(COMPACT_TOOLBAR_WIDTH),
                    wrap: status.wrap,
                    flags: Flags {
                        browsable: status.context.browsable,
                        files_focused: pane == ViewerPane::Files,
                        pending: status.context.repository_pending,
                    },
                    error: status.repository_error.clone(),
                }
            })
        }
    });
    view! {
        <div test_id="diff-toolbar" flex col flex_none track_bounds=bounds bg=theme.chrome border_b=px(1.0)
             border_color=theme.border>
            <dynamic>{bar(&handle, &model.get())}</dynamic>
        </div>
    }
}

fn bar(handle: &ReviewHandle, model: &Model) -> Div {
    let theme = Theme::current();
    let sidebar = handle.sidebar_width_rem();
    let focused = model.flags.files_focused;
    let files = model.split.then(|| {
        view! {
            <div test_id="toolbar-files" flex_none w={move || Length::Rem(sidebar.get())} h_full px=8.0 flex row
                 items_center gap=4.0 bg=theme.surface border_r=px(1.0)
                 border_color={move || { let c = theme.get(); if focused { c.accent } else { c.border } }}>
                {..files_controls(handle, model)}
            </div>
        }
    });
    view! {
        <div flex col>
            <div h=tokens::BAR_HEIGHT flex_none flex row>
                {files}
                <div flex_1 min_w=0.0 h_full px=8.0 flex row items_center gap=12.0>
                    {leading(handle, model)}
                    {trailing(handle, model)}
                </div>
            </div>
            {model.error.clone().map(|error| view! {
                <div px=8.0 pb=6.0><notification message=error tone=NoticeTone::Error/></div>
            })}
            {model.flags.pending.then(|| view! {
                <div px=12.0 pb=6.0><muted_text>"Updating repository…"</muted_text></div>
            })}
        </div>
    }
}

fn files_controls(handle: &ReviewHandle, model: &Model) -> Vec<AnyView> {
    let toggle = model
        .toolbar
        .files
        .iter()
        .find(|control| matches!(control.kind, ControlKind::Sidebar { .. }))
        .map(|toggle| control(handle, model, toggle, None));
    let switcher = model.flags.browsable.then(|| {
        let segments = [FilesMode::Diff, FilesMode::Browse].map(|mode| {
            let command = C::SetFilesMode(mode);
            let id = format!("files-mode-{}", mode.title().to_lowercase());
            let (icon, label) = match mode {
                FilesMode::Diff => (IconName::FileDiff, "Changed files"),
                FilesMode::Browse => (IconName::Folder, "All files"),
            };
            let detail = (mode == FilesMode::Diff && model.changes > 0).then(|| model.changes.to_string());
            let handle = *handle;
            view! {
                <segment id label icon detail?=detail selected={model.files_mode == mode}
                         disabled={!model.enabled(&command)}
                         on_click={move || {
                             handle.handle_command(command.clone());
                         }}/>
            }
        });
        view! { <segmented_control id="files-mode">{..segments}</segmented_control> }.into_any()
    });
    toggle.into_iter().chain(switcher).collect()
}

fn leading(handle: &ReviewHandle, model: &Model) -> Div {
    let files = if model.split { Vec::new() } else { files_controls(handle, model) };
    let history: Vec<_> = if model.compact {
        Vec::new()
    } else {
        model.toolbar.history.iter().map(|item| control(handle, model, item, None)).collect()
    };
    let target = review_target(handle, model);
    let (has_files, has_history) = (!files.is_empty(), !history.is_empty());
    view! {
        <div flex_1 min_w=0.0 overflow_hidden flex row items_center gap=2.0>
            {..files}
            {(has_files && has_history).then(divider)}
            {..history}
            {(target.is_some() && (has_files || has_history)).then(divider)}
            {target}
        </div>
    }
}

fn review_target(handle: &ReviewHandle, model: &Model) -> Option<Div> {
    let stepper = stepper(handle, model);
    let scopes = (!model.toolbar.scopes.is_empty()).then(|| {
        let segments = model.toolbar.scopes.iter().filter_map(|item| {
            let ControlKind::Scope(scope) = item.kind else {
                return None;
            };
            Some(view! {
                <segment id={format!("scope-{scope:?}")} label=scope.title() selected=item.selected
                         disabled={!model.enabled(&item.command)} on_click=run(handle, item)/>
            })
        });
        view! { <segmented_control id="scopes">{..segments}</segmented_control> }
    });
    if stepper.is_none() && scopes.is_none() {
        return None;
    }
    let both = stepper.is_some() && scopes.is_some();
    Some(view! {
        <div test_id="review-target" flex_grow=0.0 flex_shrink=1.0 min_w=0.0 overflow_hidden h=tokens::ROW_HEIGHT flex
             row items_center gap=2.0>
            {stepper}
            {both.then(divider)}
            {scopes}
        </div>
    })
}

fn stepper(handle: &ReviewHandle, model: &Model) -> Option<Div> {
    let (Some(label), [previous, next, choose]) = (&model.toolbar.page, model.toolbar.stepper.as_slice()) else {
        return None;
    };
    Some(view! {
        <div test_id="commit-stepper" flex_grow=0.0 flex_shrink=1.0 w=MAX_STEPPER_WIDTH min_w=MIN_STEPPER_WIDTH h_full
             px=2.0 flex row items_center gap=2.0 rounded=tokens::RADIUS_CONTROL bg=Theme::current().surface
             track_bounds=handle.commit_anchor()>
            {control(handle, model, previous, Some(tokens::ICON_SIZE_SMALL))}
            {commit_trigger(handle, model, label, choose)}
            {control(handle, model, next, Some(tokens::ICON_SIZE_SMALL))}
        </div>
    })
}

fn commit_trigger(handle: &ReviewHandle, model: &Model, label: &PageLabel, choose: &ToolbarControl) -> AnyView {
    let theme = Theme::current();
    let hovered = signal(false);
    let has_subject = label.subject().is_some();
    let run = run(handle, choose);
    let short_id = label.short_id().map(|short_id| {
        view! {
            <div test_id="commit-short-id" flex_none>
                <text small font=CODE_FONT no_wrap
                      color={move || { let c = theme.get(); if has_subject { c.text_muted } else { c.text } }}>
                    {short_id.to_owned()}
                </text>
            </div>
        }
    });
    let subject = label.subject().map(|subject| {
        view! {
            <div test_id="commit-subject" flex_grow=0.0 flex_shrink=1.0 min_w=0.0 overflow_hidden>
                <text body no_wrap primary>{subject.to_owned()}</text>
            </div>
        }
    });
    let position = label.position_label().filter(|_| !model.compact).map(|position| {
        view! { <div test_id="commit-position"><text small no_wrap muted>{position}</text></div> }
    });
    let trigger = view! {
        <div test_id="choose-commit" w_full min_w=0.0 h=tokens::ICON_BUTTON_SIZE px=8.0 flex row items_center gap=8.0
             rounded=tokens::RADIUS_INNER cursor=Cursor::Pointer
             bg={move || if hovered.get() { theme.surface_hover.get() } else { Color::TRANSPARENT }}
             on_hover={move |inside| {
                 hovered.set_if_changed(inside);
             }}
             on_click={move |event| {
                 event.stop_propagation();
                 run();
             }}>
            {short_id}
            {subject}
            <div flex_1/>
            <div flex_none flex row items_center gap=8.0>
                {position}
                {Icon::new(IconName::ChevronDown, theme.text_muted).size(tokens::ICON_SIZE_SMALL)}
            </div>
        </div>
    };
    trigger.tooltip(choose.tooltip.clone()).flex_1().min_w(0.0).into_any()
}

fn trailing(handle: &ReviewHandle, model: &Model) -> Div {
    let toolbar = &model.toolbar;
    let stat = toolbar.stat.filter(|_| !model.compact).map(|stat| {
        view! { <div test_id="diff-summary" flex_none px=4.0>{style::stat_badges(stat, handle.theme())}</div> }
    });
    let collapse = toolbar
        .files
        .iter()
        .find(|item| matches!(item.kind, ControlKind::AllFiles { .. }))
        .map(|item| control(handle, model, item, None));
    let layouts = (!toolbar.layouts.is_empty()).then(|| {
        let segments = toolbar.layouts.iter().map(|item| {
            let (id, icon) = control_icon(item.kind);
            view! {
                <segment id label={item.tooltip.clone()} icon selected=item.selected
                         disabled={!model.enabled(&item.command)} on_click=run(handle, item)/>
            }
        });
        view! { <segmented_control id="view-mode-toggle">{..segments}</segmented_control> }
    });
    let handle = *handle;
    let wrap = toolbar.stat.map(|_| {
        let text = if model.wrap { "Stop wrapping long lines" } else { "Wrap long lines" };
        let label = tooltip(text, shortcut_label(&C::ToggleWrap));
        view! {
            <icon_button id="toggle-wrap" icon=IconName::WrapText label selected=model.wrap
                         disabled={!model.enabled(&C::ToggleWrap)}
                         on_click={move || {
                             handle.handle_command(C::ToggleWrap);
                         }}/>
        }
    });
    let size = toolbar.stat.and_then(|_| size_menu());
    let after_stat = stat.is_some() && (collapse.is_some() || layouts.is_some() || wrap.is_some() || size.is_some());
    view! {
        <div flex_none flex row items_center justify_end gap=2.0>
            {stat}
            {after_stat.then(divider)}
            {collapse}
            {layouts}
            {wrap}
            {size}
            <icon_button id="command-palette-button" icon=IconName::Search label="Commands"
                         on_click={move || {
                             handle.handle_command(ReviewCommand::ShowHelp);
                         }}/>
        </div>
    }
}

fn size_menu() -> Option<AnyView> {
    let theme = Theme::current();
    let size = use_context::<InterfaceSize>()?;
    let open = signal(false);
    let trigger = view! {
        <button id="interface-size" label="Aa" variant=ButtonVariant::Ghost size=ControlSize::Small tooltip="Text size"
                on_click={move || open.set(true)}/>
    };
    let menu = move || {
        let step = |id: &str, icon, label: &str, action| {
            view! { <icon_button id icon label on_click={move || size.apply(action)}/> }
        };
        let hovered = signal(false);
        view! {
            <div test_id="interface-size-menu" p=tokens::FLOATING_INSET flex row items_center gap=2.0>
                <div px=4.0 mr=4.0><text small secondary>"Text size"</text></div>
                {step("decrease-interface-size", IconName::Minus, "Decrease text size", SizeAction::Decrease)}
                <div test_id="reset-interface-size" h=tokens::ICON_BUTTON_SIZE px=tokens::CONTROL_PADDING_X_SMALL flex
                     items_center rounded=tokens::RADIUS_INNER cursor=Cursor::Pointer
                     bg={move || if hovered.get() { theme.surface_hover.get() } else { Color::TRANSPARENT }}
                     on_hover={move |inside| {
                         hovered.set_if_changed(inside);
                     }}
                     on_click={move |event| {
                         event.stop_propagation();
                         size.apply(SizeAction::Reset);
                     }}>
                    <text label primary>{move || format!("{:.0}", size.get())}</text>
                </div>
                {step("increase-interface-size", IconName::Plus, "Increase text size", SizeAction::Increase)}
            </div>
        }
    };
    Some(popover(open, trigger, menu).into_any())
}

fn divider() -> Div {
    view! { <div flex_none w=px(1.0) h=tokens::ICON_SIZE mx=4.0 bg=Theme::current().border_subtle/> }
}

fn run(handle: &ReviewHandle, item: &ToolbarControl) -> impl Fn() + Clone + 'static {
    let handle = *handle;
    let command = item.command.clone();
    move || {
        handle.handle_command(command.clone());
    }
}

fn control(handle: &ReviewHandle, model: &Model, item: &ToolbarControl, icon_size: Option<f32>) -> AnyView {
    let (id, icon) = control_icon(item.kind);
    view! {
        <icon_button id icon label={item.tooltip.clone()} icon_size?=icon_size selected=item.selected
                     disabled={!model.enabled(&item.command)} on_click=run(handle, item)/>
    }
}

const fn control_icon(kind: ControlKind) -> (&'static str, IconName) {
    match kind {
        ControlKind::Sidebar { .. } => ("toggle-sidebar", IconName::PanelLeft),
        ControlKind::AllFiles { collapsed: true } => ("toggle-all-files", IconName::ChevronsUpDown),
        ControlKind::AllFiles { collapsed: false } => ("toggle-all-files", IconName::ChevronsDownUp),
        ControlKind::Back => ("navigate-back", IconName::ArrowLeft),
        ControlKind::Forward => ("navigate-forward", IconName::ArrowRight),
        ControlKind::PreviousCommit => ("previous-commit", IconName::ChevronLeft),
        ControlKind::NextCommit => ("next-commit", IconName::ChevronRight),
        ControlKind::Layout(ViewMode::Auto) => ("layout-auto", IconName::MoveHorizontal),
        ControlKind::Layout(ViewMode::Unified) => ("layout-unified", IconName::Rows2),
        ControlKind::Layout(ViewMode::Split) => ("layout-split", IconName::Columns2),
        ControlKind::ChooseCommit | ControlKind::Scope(_) => ("choose-commit", IconName::GitCommit),
    }
}
