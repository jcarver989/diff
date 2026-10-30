use crate::{
    DiffViewer,
    style::stat_badges,
    ui::{
        prelude::{
            CODE_FONT, Icon, IconButton, IconName, Segment, SegmentedControl, Tooltip, Typography, TypographyExt, rem,
        },
        tokens::{self, BAR_HEIGHT, ICON_BUTTON_SIZE, ICON_SIZE},
    },
};
use clankerdiff_core::{ControlKind, Toolbar, ToolbarControl, ViewMode, default_key_label};
use gpui::{App, ClickEvent, Context, Div, Role, SharedString, Window, div, prelude::*};

const COMPACT_TOOLBAR_WIDTH: f32 = 1000.0;
const MAX_STEPPER_WIDTH: f32 = 360.0;
const MIN_STEPPER_WIDTH: f32 = 160.0;

impl DiffViewer {
    pub(crate) fn toolbar(&self) -> Toolbar {
        Toolbar::new(self.session(), Some(self.sidebar_collapsed()), false, default_key_label)
    }

    pub(crate) fn render_toolbar(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let toolbar = self.toolbar();
        let compact = window.viewport_size().width < rem(COMPACT_TOOLBAR_WIDTH).to_pixels(window.rem_size());
        let layouts = (!toolbar.layouts.is_empty()).then(|| {
            SegmentedControl::new("view-mode-toggle", self.ui_theme()).segments(toolbar.layouts.iter().map(|control| {
                let (id, icon) = control_icon(control.kind);
                Segment::icon(id, icon, control.tooltip.clone()).selected(control.selected).on_click(run(control, cx))
            }))
        });
        div()
            .debug_selector(|| "diff-toolbar".to_owned())
            .h(rem(BAR_HEIGHT))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .border_b_1()
            .border_color(self.ui_theme().colors.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .gap_2()
                    .children(self.render_toolbar_leading(&toolbar, cx))
                    .children(self.render_review_target(&toolbar, compact, cx)),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .children(layouts)
                    .when(toolbar.stat.is_some(), |side| side.child(self.render_interface_size_controls(window, cx)))
                    .children(toolbar.stat.filter(|_| !compact).map(|stat| {
                        div()
                            .debug_selector(|| "diff-summary".to_owned())
                            .ml_2()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_2()
                            .whitespace_nowrap()
                            .children(stat_badges(&self.theme().diff, stat))
                    })),
            )
    }

    pub(crate) fn toolbar_leading_spacer(&self) -> Option<Div> {
        let toolbar = self.toolbar();
        let slot = || div().flex_none().w(rem(ICON_BUTTON_SIZE));
        (!toolbar.files.is_empty()).then(|| {
            leading_group()
                .children(toolbar.files.iter().map(|_| slot()))
                .child(divider())
                .children(toolbar.history.iter().map(|_| slot()))
        })
    }

    fn render_toolbar_leading(&self, toolbar: &Toolbar, cx: &mut Context<Self>) -> Option<Div> {
        if toolbar.history.is_empty() {
            return None;
        }
        let mut group = leading_group();
        for control in &toolbar.files {
            group = match control.kind {
                ControlKind::Sidebar { .. } => group.child(self.render_sidebar_toggle(cx)),
                _ => group.child(self.control_button(control, cx)),
            };
        }
        group = group.child(divider().bg(self.ui_theme().colors.border_subtle));
        Some(group.children(toolbar.history.iter().map(|control| self.control_button(control, cx))))
    }

    fn render_review_target(&self, toolbar: &Toolbar, compact: bool, cx: &mut Context<Self>) -> Option<Div> {
        let theme = self.ui_theme();
        let stepper = self.render_commit_stepper(toolbar, compact, cx);
        let scopes = (!toolbar.scopes.is_empty()).then(|| {
            SegmentedControl::new("scopes", theme).embedded().segments(toolbar.scopes.iter().filter_map(|control| {
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
        let divider = (stepper.is_some() && scopes.is_some())
            .then(|| div().flex_none().w_px().h(rem(ICON_SIZE)).mx_0p5().bg(theme.colors.border_subtle));
        Some(
            div()
                .debug_selector(|| "review-target".to_owned())
                .flex_initial()
                .min_w_0()
                .overflow_hidden()
                .h(rem(tokens::ROW_HEIGHT))
                .px_0p5()
                .flex()
                .items_center()
                .gap_0p5()
                .rounded(rem(tokens::RADIUS_CONTROL))
                .border_1()
                .border_color(theme.colors.border_subtle)
                .bg(theme.colors.surface)
                .shadow_sm()
                .children(stepper)
                .children(divider)
                .children(scopes),
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
            .flex_initial()
            .min_w_0()
            .h(rem(tokens::ICON_BUTTON_SIZE))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded_full()
            .cursor_pointer()
            .hover(move |trigger| trigger.bg(colors.surface_hover))
            .tooltip(Tooltip::text(tooltip, theme))
            .children(short_id)
            .children(subject)
            .children(position)
            .child(Icon::new(IconName::ChevronDown, colors.text_muted).size(tokens::ICON_SIZE_SMALL));
        Some(
            div()
                .debug_selector(|| "commit-stepper".to_owned())
                .flex_initial()
                .min_w(rem(MIN_STEPPER_WIDTH))
                .max_w(rem(MAX_STEPPER_WIDTH))
                .h_full()
                .flex()
                .items_center()
                .gap_0p5()
                .child(self.control_button(previous, cx).icon_size(tokens::ICON_SIZE_SMALL))
                .child(self.control_button(next, cx).icon_size(tokens::ICON_SIZE_SMALL))
                .child(trigger.on_click(run(choose, cx))),
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

fn leading_group() -> Div {
    div().flex_none().flex().items_center().gap_0p5()
}

fn divider() -> Div {
    div().flex_none().w_px().h(rem(ICON_SIZE)).mx_1()
}
