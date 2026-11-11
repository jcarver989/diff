use crate::{ReviewHandle, ViewerPane, shortcut_label, style};
use clankerdiff_core::{
    ControlKind, DiffReviewCommand as C, FilesMode, PageLabel, ReviewCommand, Toolbar, ToolbarControl, ViewMode,
};
use clankerdiff_rouge_ui::{
    components::{Button, ButtonVariant, ControlSize, IconButton, MutedText, Notification, Segment, SegmentedControl},
    fonts::CODE_FONT,
    icon::{Icon, IconName},
    interface_size::{InterfaceSize, SizeAction},
    theme::UiTheme,
    tokens::{self, rem, scaled},
    typography::{Typography, TypographyExt},
};
use rouge_core::{AnyView, Cursor, Div, Length, Rect, Styled, View, div, dynamic, root_font_size, text};
use rouge_reactive::{memo, signal, use_context};

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

pub(crate) fn toolbar(handle: &ReviewHandle, theme: UiTheme) -> impl View {
    let status = handle.status();
    let revision = handle.presentation_revision();
    let selection = handle.selection();
    let sidebar = handle.sidebar_width_rem();
    let bounds = signal(Rect::ZERO);
    let font = root_font_size();
    let model = memo({
        let handle = handle.clone();
        move || {
            let status = status.get();
            revision.get();
            let pane = selection.get().pane;
            let width = bounds.get().size.width / font.get();
            let px = |design: f32| design / tokens::BASE_SIZE;
            let beside = width - sidebar.get();
            let split = !status.sidebar_collapsed && beside >= px(MIN_SPLIT_TOOLBAR_WIDTH);
            let room = if split { beside } else { width };
            handle.with(|state| {
                let session = state.session();
                let toolbar =
                    Toolbar::new(session, Some(status.sidebar_collapsed), status.context.browsable, shortcut_label);
                let enabled = [&toolbar.files, &toolbar.history, &toolbar.stepper, &toolbar.scopes, &toolbar.layouts]
                    .into_iter()
                    .flatten()
                    .map(|control| control.command.clone())
                    .chain([C::SetFilesMode(FilesMode::Diff), C::SetFilesMode(FilesMode::Browse)])
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
                    compact: room < px(COMPACT_TOOLBAR_WIDTH),
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
    let bar = dynamic({
        let handle = handle.clone();
        move || bar(&handle, &model.get(), theme)
    });
    div()
        .test_id("diff-toolbar")
        .flex()
        .col()
        .flex_none()
        .track_bounds(bounds)
        .bg(theme.paint(|c| c.chrome))
        .border_b(1.0)
        .border_color(theme.paint(|c| c.border))
        .child(bar)
}

fn bar(handle: &ReviewHandle, model: &Model, theme: UiTheme) -> Div {
    let sidebar = handle.sidebar_width_rem();
    let mut row = div().h(rem(tokens::BAR_HEIGHT)).flex_none().flex().row();
    if model.split {
        row = row.child(
            div()
                .test_id("toolbar-files")
                .flex_none()
                .w(move || Length::Rem(sidebar.get()))
                .h_full()
                .px(rem(8.0))
                .flex()
                .row()
                .items_center()
                .gap(rem(4.0))
                .bg(theme.paint(|c| c.surface))
                .border_r(1.0)
                .border_color({
                    let focused = model.flags.files_focused;
                    theme.paint(move |c| if focused { c.accent } else { c.border })
                })
                .children(files_controls(handle, model, theme)),
        );
    }
    row = row.child(
        div()
            .flex_1()
            .min_w(0.0)
            .h_full()
            .px(rem(8.0))
            .flex()
            .row()
            .items_center()
            .gap(rem(12.0))
            .child(leading(handle, model, theme))
            .child(trailing(handle, model, theme)),
    );
    let mut bar = div().flex().col().child(row);
    if let Some(error) = model.error.clone() {
        bar = bar.child(div().px(rem(8.0)).pb(rem(6.0)).child(Notification::error(error, theme)));
    }
    if model.flags.pending {
        bar = bar.child(div().px(rem(12.0)).pb(rem(6.0)).child(MutedText::new("Updating repository…", theme)));
    }
    bar
}

fn files_controls(handle: &ReviewHandle, model: &Model, theme: UiTheme) -> Vec<AnyView> {
    let mut controls = Vec::new();
    if let Some(toggle) = model.toolbar.files.iter().find(|control| matches!(control.kind, ControlKind::Sidebar { .. }))
    {
        controls.push(control(handle, model, toggle, theme));
    }
    if model.flags.browsable {
        controls.push(
            SegmentedControl::new("files-mode", theme)
                .segments([FilesMode::Diff, FilesMode::Browse].map(|mode| {
                    let command = C::SetFilesMode(mode);
                    let id = format!("files-mode-{}", mode.title().to_lowercase());
                    let segment = match mode {
                        FilesMode::Diff if model.changes > 0 => {
                            Segment::icon(id, IconName::FileDiff, "Changed files").detail(model.changes.to_string())
                        }
                        FilesMode::Diff => Segment::icon(id, IconName::FileDiff, "Changed files"),
                        FilesMode::Browse => Segment::icon(id, IconName::Folder, "All files"),
                    };
                    segment.selected(model.files_mode == mode).disabled(!model.enabled(&command)).on_click({
                        let handle = handle.clone();
                        move || {
                            handle.handle_command(command.clone());
                        }
                    })
                }))
                .into_any(),
        );
    }
    controls
}

fn leading(handle: &ReviewHandle, model: &Model, theme: UiTheme) -> Div {
    let files = if model.split { Vec::new() } else { files_controls(handle, model, theme) };
    let history: Vec<_> = if model.compact {
        Vec::new()
    } else {
        model.toolbar.history.iter().map(|item| control(handle, model, item, theme)).collect()
    };
    let target = review_target(handle, model, theme);
    let mut leading = div().flex_1().min_w(0.0).overflow_hidden().flex().row().items_center().gap(rem(2.0));
    let has_files = !files.is_empty();
    let has_history = !history.is_empty();
    leading = leading.children(files);
    if has_files && has_history {
        leading = leading.child(divider(theme));
    }
    leading = leading.children(history);
    if let Some(target) = target {
        if has_files || has_history {
            leading = leading.child(divider(theme));
        }
        leading = leading.child(target);
    }
    leading
}

fn review_target(handle: &ReviewHandle, model: &Model, theme: UiTheme) -> Option<Div> {
    let stepper = stepper(handle, model, theme);
    let scopes = (!model.toolbar.scopes.is_empty()).then(|| {
        SegmentedControl::new("scopes", theme).segments(model.toolbar.scopes.iter().filter_map(|item| {
            let ControlKind::Scope(scope) = item.kind else {
                return None;
            };
            Some(
                Segment::new(format!("scope-{scope:?}"), scope.title())
                    .selected(item.selected)
                    .disabled(!model.enabled(&item.command))
                    .on_click(run(handle, item)),
            )
        }))
    });
    if stepper.is_none() && scopes.is_none() {
        return None;
    }
    let mut target = div()
        .test_id("review-target")
        .flex_grow(0.0)
        .flex_shrink(1.0)
        .min_w(0.0)
        .overflow_hidden()
        .h(rem(tokens::ROW_HEIGHT))
        .flex()
        .row()
        .items_center()
        .gap(rem(2.0));
    let both = stepper.is_some() && scopes.is_some();
    if let Some(stepper) = stepper {
        target = target.child(stepper);
    }
    if both {
        target = target.child(divider(theme));
    }
    if let Some(scopes) = scopes {
        target = target.child(scopes);
    }
    Some(target)
}

fn stepper(handle: &ReviewHandle, model: &Model, theme: UiTheme) -> Option<Div> {
    let (Some(label), [previous, next, choose]) = (&model.toolbar.page, model.toolbar.stepper.as_slice()) else {
        return None;
    };
    let trigger = commit_trigger(handle, model, label, choose, theme);
    Some(
        div()
            .test_id("commit-stepper")
            .flex_grow(0.0)
            .flex_shrink(1.0)
            .w(rem(MAX_STEPPER_WIDTH))
            .min_w(rem(MIN_STEPPER_WIDTH))
            .h_full()
            .px(rem(2.0))
            .flex()
            .row()
            .items_center()
            .gap(rem(2.0))
            .rounded(scaled(tokens::RADIUS_CONTROL))
            .bg(theme.paint(|c| c.surface))
            .track_bounds(handle.commit_anchor())
            .child(icon_control(handle, model, previous, theme).icon_size(tokens::ICON_SIZE_SMALL))
            .child(trigger)
            .child(icon_control(handle, model, next, theme).icon_size(tokens::ICON_SIZE_SMALL)),
    )
}

fn commit_trigger(
    handle: &ReviewHandle,
    model: &Model,
    label: &PageLabel,
    choose: &ToolbarControl,
    theme: UiTheme,
) -> AnyView {
    let hovered = signal(false);
    let has_subject = label.subject().is_some();
    let mut trigger = div()
        .test_id("choose-commit")
        .w_full()
        .min_w(0.0)
        .h(rem(tokens::ICON_BUTTON_SIZE))
        .px(rem(8.0))
        .flex()
        .row()
        .items_center()
        .gap(rem(8.0))
        .rounded(scaled(tokens::RADIUS_INNER))
        .cursor(Cursor::Pointer)
        .bg(theme.paint(move |c| if hovered.get() { c.surface_hover } else { rouge_core::Color::TRANSPARENT }))
        .on_hover(move |inside| {
            hovered.set_if_changed(inside);
        })
        .on_click({
            let run = run(handle, choose);
            move |event| {
                event.stop_propagation();
                run();
            }
        });
    if let Some(short_id) = label.short_id() {
        trigger = trigger.child(
            div().test_id("commit-short-id").flex_none().child(
                text(short_id.to_owned())
                    .typography(Typography::Small)
                    .font(CODE_FONT)
                    .no_wrap()
                    .color(theme.paint(move |c| if has_subject { c.text_muted } else { c.text })),
            ),
        );
    }
    if let Some(subject) = label.subject() {
        trigger =
            trigger.child(
                div().test_id("commit-subject").flex_grow(0.0).flex_shrink(1.0).min_w(0.0).overflow_hidden().child(
                    text(subject.to_owned()).typography(Typography::Body).no_wrap().color(theme.paint(|c| c.text)),
                ),
            );
    }
    let mut end = div().flex_none().flex().row().items_center().gap(rem(8.0));
    if let Some(position) = label.position_label().filter(|_| !model.compact) {
        end = end.child(
            div()
                .test_id("commit-position")
                .child(text(position).typography(Typography::Small).no_wrap().color(theme.paint(|c| c.text_muted))),
        );
    }
    trigger = trigger.child(div().flex_1()).child(
        end.child(Icon::new(IconName::ChevronDown, theme.paint(|c| c.text_muted)).size(tokens::ICON_SIZE_SMALL)),
    );
    rouge_widgets::tooltip(trigger, choose.tooltip.clone()).flex_1().min_w(0.0).into_any()
}

fn trailing(handle: &ReviewHandle, model: &Model, theme: UiTheme) -> Div {
    let toolbar = &model.toolbar;
    let stat = toolbar.stat.filter(|_| !model.compact).map(|stat| {
        div().test_id("diff-summary").flex_none().px(rem(4.0)).child(style::stat_badges(stat, handle.theme()))
    });
    let collapse = toolbar
        .files
        .iter()
        .find(|item| matches!(item.kind, ControlKind::AllFiles { .. }))
        .map(|item| control(handle, model, item, theme));
    let layouts = (!toolbar.layouts.is_empty()).then(|| {
        SegmentedControl::new("view-mode-toggle", theme).segments(toolbar.layouts.iter().map(|item| {
            let (id, icon) = control_icon(item.kind);
            Segment::icon(id, icon, item.tooltip.clone())
                .selected(item.selected)
                .disabled(!model.enabled(&item.command))
                .on_click(run(handle, item))
        }))
    });
    let size = toolbar.stat.and_then(|_| size_menu(theme));
    let mut trailing = div().flex_none().flex().row().items_center().justify_end().gap(rem(2.0));
    let after_stat = stat.is_some() && (collapse.is_some() || layouts.is_some() || size.is_some());
    if let Some(stat) = stat {
        trailing = trailing.child(stat);
    }
    if after_stat {
        trailing = trailing.child(divider(theme));
    }
    if let Some(collapse) = collapse {
        trailing = trailing.child(collapse);
    }
    if let Some(layouts) = layouts {
        trailing = trailing.child(layouts);
    }
    if let Some(size) = size {
        trailing = trailing.child(size);
    }
    trailing.child(IconButton::new("command-palette-button", IconName::Search, "Commands", theme).on_click({
        let handle = handle.clone();
        move || {
            handle.handle_command(ReviewCommand::ShowHelp);
        }
    }))
}

fn size_menu(theme: UiTheme) -> Option<AnyView> {
    let size = use_context::<InterfaceSize>()?;
    let open = signal(false);
    let trigger = Button::new("interface-size", "Aa", theme)
        .variant(ButtonVariant::Ghost)
        .size(ControlSize::Small)
        .tooltip("Text size")
        .on_click(move || open.set(true));
    let menu = move || {
        let step = |id: &str, icon, label: &str, action| {
            IconButton::new(id, icon, label, theme).on_click(move || size.apply(action))
        };
        let hovered = signal(false);
        div()
            .test_id("interface-size-menu")
            .p(rem(tokens::FLOATING_INSET))
            .flex()
            .row()
            .items_center()
            .gap(rem(2.0))
            .child(
                div()
                    .px(rem(4.0))
                    .mr(rem(4.0))
                    .child(text("Text size").typography(Typography::Small).color(theme.paint(|c| c.text_secondary))),
            )
            .child(step("decrease-interface-size", IconName::Minus, "Decrease text size", SizeAction::Decrease))
            .child(
                div()
                    .test_id("reset-interface-size")
                    .h(rem(tokens::ICON_BUTTON_SIZE))
                    .px(rem(tokens::CONTROL_PADDING_X_SMALL))
                    .flex()
                    .items_center()
                    .rounded(scaled(tokens::RADIUS_INNER))
                    .cursor(Cursor::Pointer)
                    .bg(theme
                        .paint(move |c| if hovered.get() { c.surface_hover } else { rouge_core::Color::TRANSPARENT }))
                    .on_hover(move |inside| {
                        hovered.set_if_changed(inside);
                    })
                    .on_click(move |event| {
                        event.stop_propagation();
                        size.apply(SizeAction::Reset);
                    })
                    .child(
                        text(move || format!("{:.0}", size.get()))
                            .typography(Typography::Label)
                            .color(theme.paint(|c| c.text)),
                    ),
            )
            .child(step("increase-interface-size", IconName::Plus, "Increase text size", SizeAction::Increase))
    };
    Some(rouge_widgets::popover(open, trigger, menu).into_any())
}

fn divider(theme: UiTheme) -> Div {
    div().flex_none().w(1.0).h(rem(tokens::ICON_SIZE)).mx(rem(4.0)).bg(theme.paint(|c| c.border_subtle))
}

fn run(handle: &ReviewHandle, item: &ToolbarControl) -> impl Fn() + Clone + 'static {
    let handle = handle.clone();
    let command = item.command.clone();
    move || {
        handle.handle_command(command.clone());
    }
}

fn icon_control(handle: &ReviewHandle, model: &Model, item: &ToolbarControl, theme: UiTheme) -> IconButton {
    let (id, icon) = control_icon(item.kind);
    IconButton::new(id, icon, item.tooltip.clone(), theme)
        .selected(item.selected)
        .disabled(!model.enabled(&item.command))
        .on_click(run(handle, item))
}

fn control(handle: &ReviewHandle, model: &Model, item: &ToolbarControl, theme: UiTheme) -> AnyView {
    icon_control(handle, model, item, theme).into_any()
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
