use crate::{
    DiffViewer,
    comment_editor::init_input,
    style::color,
    ui::{
        prelude::{
            CODE_FONT, Icon, IconButton, IconName, PickerDetail, PickerItem, SearchPicker, SearchPickerEvent, Segment,
            SegmentedControl, Tooltip, Typography, TypographyExt, UiTheme, rem,
        },
        tokens,
    },
};
use clankerdiff_core::{CommitPage, CommitPicker, DiffReviewCommand, DiffScope, PageLabel, ReviewCommand};
use clankerdiff_theme::DiffPalette;
use gpui::{Context, Div, Entity, MouseButton, Role, Subscription, Window, div, prelude::*};
use web_time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_STEPPER_WIDTH: f32 = 360.0;
const MIN_STEPPER_WIDTH: f32 = 160.0;

pub(crate) struct CommitPickerOverlay {
    picker: Entity<SearchPicker>,
    _subscription: Subscription,
}

impl CommitPickerOverlay {
    pub(crate) fn move_highlight(&self, by: isize, cx: &mut Context<DiffViewer>) {
        self.picker.update(cx, |picker, cx| picker.move_highlight(by, cx));
    }
}

impl DiffViewer {
    pub(crate) fn open_commit_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(commits) = self.session().commit_pages(since_epoch()).and_then(CommitPicker::new) else {
            return;
        };
        init_input(cx);
        let theme = self.ui_theme();
        let items = commit_items(commits.pages(), &self.theme().diff, &theme);
        let picker = cx.new(|cx| {
            SearchPicker::new("commit-picker", items, theme, cx)
                .width(rem(tokens::PICKER_WIDTH))
                .max_list_height(rem(tokens::PICKER_LIST_HEIGHT))
                .highlight(commits.highlighted())
        });
        let subscription =
            cx.subscribe_in(&picker, window, |viewer, _, event: &SearchPickerEvent, window, cx| match event {
                SearchPickerEvent::Chose(index) => {
                    viewer.handle_command(DiffReviewCommand::GoToPage(*index), window, cx);
                }
                SearchPickerEvent::Dismissed => {
                    viewer.handle_command(ReviewCommand::Cancel, window, cx);
                }
                SearchPickerEvent::Highlighted(_) | SearchPickerEvent::Query(_) => {}
            });
        self.commit_picker = Some(CommitPickerOverlay { picker, _subscription: subscription });
    }

    pub(crate) fn render_review_target(&self, compact: bool, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let theme = self.ui_theme();
        let stepper = self.render_commit_stepper(compact, cx);
        let scopes = self.session().target().scope().map(|selected| self.render_scopes(selected, cx));
        if stepper.is_none() && scopes.is_none() {
            return None;
        }
        let divider = (stepper.is_some() && scopes.is_some())
            .then(|| div().flex_none().w_px().h(rem(tokens::ICON_SIZE)).mx_0p5().bg(theme.colors.border_subtle));
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

    fn render_commit_stepper(&self, compact: bool, cx: &mut Context<Self>) -> Option<Div> {
        let label = self.session().page_label().filter(PageLabel::is_paged)?;
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
        let trigger = div()
            .id("commit-page")
            .debug_selector(|| "commit-page".to_owned())
            .role(Role::Button)
            .aria_label("Choose commit (p)")
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
            .tooltip(Tooltip::text("Choose commit (p)", theme))
            .on_click(cx.listener(|viewer, _, window, cx| {
                viewer.handle_command(DiffReviewCommand::OpenCommitPicker, window, cx);
            }))
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
                .child(self.step_button("commit-previous", IconName::ChevronLeft, "Previous commit ([)", -1, cx))
                .child(self.step_button("commit-next", IconName::ChevronRight, "Next commit (])", 1, cx))
                .child(trigger),
        )
    }

    fn render_scopes(&self, selected: DiffScope, cx: &mut Context<Self>) -> SegmentedControl {
        SegmentedControl::new("scopes", self.ui_theme()).embedded().segments(DiffScope::ALL.map(|scope| {
            let command = DiffReviewCommand::SetScope(scope);
            Segment::new(format!("scope-{scope}"), scope.title())
                .disabled(!self.command_enabled(&command))
                .selected(selected == scope)
                .on_click(cx.listener(move |viewer, _, window, cx| {
                    viewer.handle_command(command.clone(), window, cx);
                }))
        }))
    }

    pub(crate) fn render_commit_picker(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let picker = self.commit_picker.as_ref()?.picker.clone();
        Some(
            div()
                .id("commit-picker-backdrop")
                .debug_selector(|| "commit-picker-backdrop".to_owned())
                .absolute()
                .inset_0()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|viewer, _, window, cx| {
                        cx.stop_propagation();
                        viewer.handle_command(ReviewCommand::Cancel, window, cx);
                    }),
                )
                .child(
                    div()
                        .absolute()
                        .top(rem(tokens::BAR_HEIGHT))
                        .left_0()
                        .right_0()
                        .px_3()
                        .flex()
                        .gap_2()
                        .children(self.toolbar_leading_spacer())
                        .child(picker),
                ),
        )
    }

    fn step_button(
        &self,
        id: &'static str,
        icon: IconName,
        label: &'static str,
        delta: isize,
        cx: &mut Context<Self>,
    ) -> IconButton {
        let command = DiffReviewCommand::StepTarget(delta);
        IconButton::new(id, icon, label, self.ui_theme())
            .icon_size(tokens::ICON_SIZE_SMALL)
            .disabled(!self.command_enabled(&command))
            .on_click(cx.listener(move |viewer, _, window, cx| {
                viewer.handle_command(command.clone(), window, cx);
            }))
    }
}

fn commit_items(pages: &[CommitPage], palette: &DiffPalette, theme: &UiTheme) -> Vec<PickerItem> {
    pages
        .iter()
        .map(|page| {
            let mut item = PickerItem::new(format!("page-{}", page.number), page.subject.clone()).checked(page.current);
            if let Some(short_id) = &page.short_id {
                item = item.prefix(short_id.clone());
            }
            if let Some(age) = &page.age {
                item = item.detail(age.clone());
            }
            if page.comments > 0 {
                item = item.detail(PickerDetail::new(format!("{} 💬", page.comments)).color(theme.colors.accent));
            }
            if let Some(stat) = page.stat {
                item = item
                    .detail(PickerDetail::new(format!("+{}", stat.added)).code().color(color(palette.addition)))
                    .detail(PickerDetail::new(format!("−{}", stat.deleted)).code().color(color(palette.deletion)));
            }
            item
        })
        .collect()
}

fn since_epoch() -> Duration {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default()
}
