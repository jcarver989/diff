use crate::{
    DiffViewer, ViewerPane,
    style::stat_badges,
    ui::{
        prelude::{
            CODE_FONT, Icon, IconButton, IconName, Segment, SegmentedControl, Tooltip, Typography, TypographyExt, rem,
        },
        tokens::{self, BAR_HEIGHT, ICON_SIZE},
    },
};
use clankerdiff_core::{
    ControlKind, DiffReviewCommand, FilesMode, Toolbar, ToolbarControl, ViewMode, default_key_label,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, Hsla, Role, SharedString, Window, anchored, deferred, div, prelude::*,
    relative,
};

const COMPACT_TOOLBAR_WIDTH: f32 = 900.0;
const MIN_SPLIT_TOOLBAR_WIDTH: f32 = 640.0;
const MAX_STEPPER_WIDTH: f32 = 360.0;
const MIN_STEPPER_WIDTH: f32 = 160.0;

impl DiffViewer {
    pub(crate) fn toolbar(&self) -> Toolbar {
        Toolbar::new(self.session(), Some(self.sidebar_collapsed()), self.browsable(), default_key_label)
    }

    pub(crate) fn render_toolbar(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let toolbar = self.toolbar();
        let rem_size = window.rem_size();
        let viewport = window.viewport_size().width;
        let beside_sidebar = viewport - self.sidebar_width().to_pixels(rem_size);
        let split = self.sidebar_visible() && beside_sidebar >= rem(MIN_SPLIT_TOOLBAR_WIDTH).to_pixels(rem_size);
        let room = if split { beside_sidebar } else { viewport };
        let compact = room < rem(COMPACT_TOOLBAR_WIDTH).to_pixels(rem_size);
        let column = split.then(|| self.render_files_column(&toolbar, cx));
        div()
            .debug_selector(|| "diff-toolbar".to_owned())
            .h(rem(BAR_HEIGHT))
            .flex_shrink_0()
            .flex()
            .border_b_1()
            .border_color(self.ui_theme().colors.border)
            .children(column)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.render_toolbar_leading(&toolbar, split, compact, cx))
                    .child(self.render_toolbar_trailing(&toolbar, compact, window, cx)),
            )
    }

    fn render_files_column(&self, toolbar: &Toolbar, cx: &mut Context<Self>) -> Div {
        let colors = self.ui_theme().colors;
        div()
            .debug_selector(|| "toolbar-files".to_owned())
            .flex_none()
            .w(self.sidebar_width())
            .h_full()
            .px_2()
            .flex()
            .items_center()
            .gap_1()
            .bg(colors.surface)
            .border_r_1()
            .border_color(if self.pane == ViewerPane::Files { colors.accent } else { colors.border })
            .children(self.render_files_controls(toolbar, cx))
    }

    fn render_files_controls(&self, toolbar: &Toolbar, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let toggle = toolbar.files.iter().find(|control| matches!(control.kind, ControlKind::Sidebar { .. }));
        toggle
            .map(|_| self.render_sidebar_toggle(cx).into_any_element())
            .into_iter()
            .chain(self.render_files_modes(cx).map(IntoElement::into_any_element))
            .collect()
    }

    fn render_toolbar_leading(&self, toolbar: &Toolbar, split: bool, compact: bool, cx: &mut Context<Self>) -> Div {
        let files = if split { Vec::new() } else { self.render_files_controls(toolbar, cx) };
        let history: Vec<_> =
            toolbar.history.iter().filter(|_| !compact).map(|control| self.control_button(control, cx)).collect();
        let target = self.render_review_target(toolbar, compact, cx);
        let theme = self.ui_theme();
        let before_history = (!files.is_empty() && !history.is_empty()).then(|| divider(theme.colors.border_subtle));
        let before_target = (!(files.is_empty() && history.is_empty()) && target.is_some())
            .then(|| divider(theme.colors.border_subtle));
        div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .flex()
            .items_center()
            .gap_0p5()
            .children(files)
            .children(before_history)
            .children(history)
            .children(before_target)
            .children(target)
    }

    fn render_toolbar_trailing(
        &self,
        toolbar: &Toolbar,
        compact: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let stat = toolbar.stat.filter(|_| !compact).map(|stat| {
            div()
                .debug_selector(|| "diff-summary".to_owned())
                .flex_none()
                .px_1()
                .flex()
                .items_center()
                .gap_2()
                .whitespace_nowrap()
                .children(stat_badges(&self.theme().diff, stat))
        });
        let collapse = toolbar
            .files
            .iter()
            .find(|control| matches!(control.kind, ControlKind::AllFiles { .. }))
            .map(|control| self.control_button(control, cx));
        let layouts = (!toolbar.layouts.is_empty()).then(|| {
            SegmentedControl::new("view-mode-toggle", self.ui_theme()).segments(toolbar.layouts.iter().map(|control| {
                let (id, icon) = control_icon(control.kind);
                Segment::icon(id, icon, control.tooltip.clone()).selected(control.selected).on_click(run(control, cx))
            }))
        });
        let size = toolbar.stat.is_some().then(|| self.render_interface_size_menu(window, cx));
        let after_stat = (stat.is_some() && (collapse.is_some() || layouts.is_some() || size.is_some()))
            .then(|| divider(self.ui_theme().colors.border_subtle));
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap_0p5()
            .children(stat)
            .children(after_stat)
            .children(collapse)
            .children(layouts)
            .children(size)
    }

    fn render_files_modes(&self, cx: &mut Context<Self>) -> Option<SegmentedControl> {
        if !self.browsable() {
            return None;
        }
        let changes = self.document().files.len();
        let segments = [FilesMode::Diff, FilesMode::Browse].map(|mode| {
            let id = format!("files-mode-{}", mode.title().to_lowercase());
            let segment = match mode {
                FilesMode::Diff => {
                    let segment = Segment::icon(id, IconName::FileDiff, "Changed files");
                    if changes > 0 { segment.detail(changes.to_string()) } else { segment }
                }
                FilesMode::Browse => Segment::icon(id, IconName::Folder, "All files"),
            };
            segment.selected(self.files_mode() == mode).on_click(cx.listener(
                move |viewer, _: &ClickEvent, window, cx| {
                    viewer.handle_command(DiffReviewCommand::SetFilesMode(mode), window, cx);
                },
            ))
        });
        Some(SegmentedControl::new("files-mode", self.ui_theme()).segments(segments))
    }

    fn render_review_target(&self, toolbar: &Toolbar, compact: bool, cx: &mut Context<Self>) -> Option<Div> {
        let theme = self.ui_theme();
        let stepper = self.render_commit_stepper(toolbar, compact, cx);
        let scopes = (!toolbar.scopes.is_empty()).then(|| {
            SegmentedControl::new("scopes", theme).segments(toolbar.scopes.iter().filter_map(|control| {
                let ControlKind::Scope(scope) = control.kind else {
                    return None;
                };
                let segment = Segment::new(format!("scope-{scope}"), scope.title());
                let enabled = self.command_enabled(&control.command);
                Some(segment.disabled(!enabled).selected(control.selected).on_click(run(control, cx)))
            }))
        });
        if stepper.is_none() && scopes.is_none() {
            return None;
        }
        let between = (stepper.is_some() && scopes.is_some()).then(|| divider(theme.colors.border_subtle));
        let picker = self.commit_picker().map(|picker| {
            div()
                .absolute()
                .left_0()
                .top(relative(1.0))
                .child(deferred(anchored().child(div().pt_1().child(picker))).with_priority(1))
        });
        Some(
            div()
                .debug_selector(|| "review-target".to_owned())
                .flex_initial()
                .min_w_0()
                .overflow_hidden()
                .h(rem(tokens::ROW_HEIGHT))
                .flex()
                .items_center()
                .gap_0p5()
                .children(stepper)
                .children(between)
                .children(scopes)
                .children(picker),
        )
    }

    fn render_commit_stepper(&self, toolbar: &Toolbar, compact: bool, cx: &mut Context<Self>) -> Option<Div> {
        let (Some(label), [previous, next, choose]) = (&toolbar.page, toolbar.stepper.as_slice()) else {
            return None;
        };
        let theme = self.ui_theme();
        let colors = theme.colors;
        let short_id = label.short_id().map(|short_id| {
            div()
                .debug_selector(|| "commit-short-id".to_owned())
                .flex_none()
                .font_family(CODE_FONT)
                .typography(Typography::Small)
                .text_color(if label.subject().is_some() { colors.text_muted } else { colors.text })
                .child(short_id.to_owned())
        });
        let subject = label.subject().map(|subject| {
            div()
                .debug_selector(|| "commit-subject".to_owned())
                .flex_initial()
                .min_w_0()
                .truncate()
                .typography(Typography::Body)
                .text_color(colors.text)
                .child(subject.to_owned())
        });
        let position = label.position_label().filter(|_| !compact).map(|position| {
            div()
                .debug_selector(|| "commit-position".to_owned())
                .flex_none()
                .typography(Typography::Small)
                .text_color(colors.text_muted)
                .child(position)
        });
        let tooltip = SharedString::from(choose.tooltip.clone());
        let trigger = div()
            .id("commit-page")
            .debug_selector(|| "commit-page".to_owned())
            .role(Role::Button)
            .aria_label(tooltip.clone())
            .flex_1()
            .min_w_0()
            .h(rem(tokens::ICON_BUTTON_SIZE))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded(rem(tokens::RADIUS_INNER))
            .cursor_pointer()
            .hover(move |trigger| trigger.bg(colors.surface_hover))
            .tooltip(Tooltip::text(tooltip, theme))
            .children(short_id)
            .children(subject)
            .child(
                div()
                    .flex_none()
                    .ml_auto()
                    .flex()
                    .items_center()
                    .gap_2()
                    .children(position)
                    .child(Icon::new(IconName::ChevronDown, colors.text_muted).size(tokens::ICON_SIZE_SMALL)),
            );
        Some(
            div()
                .debug_selector(|| "commit-stepper".to_owned())
                .flex_initial()
                .w(rem(MAX_STEPPER_WIDTH))
                .min_w(rem(MIN_STEPPER_WIDTH))
                .h_full()
                .px_0p5()
                .flex()
                .items_center()
                .gap_0p5()
                .rounded(rem(tokens::RADIUS_CONTROL))
                .bg(colors.surface)
                .child(self.control_button(previous, cx).icon_size(tokens::ICON_SIZE_SMALL))
                .child(trigger.on_click(run(choose, cx)))
                .child(self.control_button(next, cx).icon_size(tokens::ICON_SIZE_SMALL)),
        )
    }

    fn control_button(&self, control: &ToolbarControl, cx: &mut Context<Self>) -> IconButton {
        let (id, icon) = control_icon(control.kind);
        IconButton::new(id, icon, control.tooltip.clone(), self.ui_theme())
            .disabled(!self.command_enabled(&control.command))
            .on_click(run(control, cx))
    }
}

fn run(
    control: &ToolbarControl,
    cx: &mut Context<DiffViewer>,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let command = control.command.clone();
    cx.listener(move |viewer, _: &ClickEvent, window, cx| {
        viewer.handle_command(command.clone(), window, cx);
    })
}

fn control_icon(kind: ControlKind) -> (&'static str, IconName) {
    match kind {
        ControlKind::AllFiles { collapsed: true } => ("toggle-all-files-collapsed", IconName::ChevronsUpDown),
        ControlKind::AllFiles { collapsed: false } => ("toggle-all-files-collapsed", IconName::ChevronsDownUp),
        ControlKind::Back => ("navigate-back", IconName::ArrowLeft),
        ControlKind::Forward => ("navigate-forward", IconName::ArrowRight),
        ControlKind::PreviousCommit => ("commit-previous", IconName::ChevronLeft),
        ControlKind::NextCommit => ("commit-next", IconName::ChevronRight),
        ControlKind::Layout(ViewMode::Auto) => ("view-mode-auto", IconName::MoveHorizontal),
        ControlKind::Layout(ViewMode::Unified) => ("view-mode-unified", IconName::Rows2),
        ControlKind::Layout(ViewMode::Split) => ("view-mode-split", IconName::Columns2),
        ControlKind::Sidebar { .. } | ControlKind::ChooseCommit | ControlKind::Scope(_) => {
            ("toolbar-control", IconName::ChevronDown)
        }
    }
}

fn divider(color: Hsla) -> Div {
    div().flex_none().w_px().h(rem(ICON_SIZE)).mx_1().bg(color)
}
