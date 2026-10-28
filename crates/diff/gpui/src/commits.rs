use crate::{
    DiffViewer,
    comment_editor::init_input,
    markdown_viewer::inline::{InlineColors, inline_text},
    style::{color, stat_badges},
    ui::{
        prelude::{
            CODE_FONT, Icon, IconButton, IconName, PickerItem, PickerMarker, SearchPicker, SearchPickerEvent, Segment,
            SegmentedControl, Tooltip, Typography, TypographyExt, UiColors, UiTheme, rem,
        },
        tokens,
    },
};
use clankerdiff_core::{
    CommitKind, CommitPage, CommitPicker, DiffReviewCommand, DiffScope, LineStat, PageLabel, ReviewCommand,
};
use clankerdiff_markdown::{MarkdownBlock, MarkdownBlockKind, MarkdownDocument};
use clankerdiff_theme::DiffPalette;
use gpui::{
    AnyElement, Context, Div, Entity, FontWeight, Hsla, MouseButton, Role, SharedString, Subscription, Window, div,
    prelude::*, relative,
};
use std::rc::Rc;
use web_time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_STEPPER_WIDTH: f32 = 360.0;
const MIN_STEPPER_WIDTH: f32 = 160.0;
const PREVIEW_PADDING_X: f32 = 24.0;
const PREVIEW_PADDING_TOP: f32 = 24.0;
const PREVIEW_PADDING_BOTTOM: f32 = 20.0;
const PREVIEW_GAP: f32 = 14.0;
const FACTS_GAP: f32 = 10.0;
const CHANGE_BAR_HEIGHT: f32 = 4.0;
const CHANGE_BAR_GAP: f32 = 2.0;

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
        let items = commit_items(commits.pages(), &theme.colors);
        let pages: Rc<[CommitPage]> = commits.pages().into();
        let preview = CommitPreview {
            theme,
            palette: self.theme().diff.clone(),
            inline: InlineColors::new(self.theme(), &theme),
        };
        let picker = cx.new(|cx| {
            SearchPicker::new("commit-picker", items, theme, cx)
                .width(rem(tokens::PICKER_PREVIEW_WIDTH))
                .height(rem(tokens::PICKER_PREVIEW_HEIGHT))
                .highlight(commits.highlighted())
                .preview(move |index, _, _| {
                    pages.get(index).map_or_else(|| div().into_any_element(), |page| preview.render(page))
                })
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

fn commit_items(pages: &[CommitPage], colors: &UiColors) -> Vec<PickerItem> {
    pages
        .iter()
        .map(|page| {
            let item = PickerItem::new(format!("page-{}", page.number), page.title.summary.clone())
                .marker(commit_marker(page, colors))
                .keywords(format!("{} {}", page.subject, page.id_label()))
                .checked(page.current);
            match &page.title.scope {
                Some(scope) => item.prefix(scope.clone()),
                None => item,
            }
        })
        .collect()
}

fn commit_marker(page: &CommitPage, colors: &UiColors) -> PickerMarker {
    if page.is_working_tree() { PickerMarker::Pending } else { PickerMarker::Dot(kind_color(page.title.kind, colors)) }
}

fn kind_color(kind: Option<CommitKind>, colors: &UiColors) -> Hsla {
    match kind {
        Some(CommitKind::Feature) => colors.accent,
        Some(CommitKind::Fix) => colors.destructive,
        Some(CommitKind::Performance | CommitKind::Revert) => colors.warning,
        Some(CommitKind::Refactor) => colors.text_secondary,
        Some(
            CommitKind::Docs
            | CommitKind::Test
            | CommitKind::Build
            | CommitKind::Ci
            | CommitKind::Chore
            | CommitKind::Style,
        )
        | None => colors.text_muted,
    }
}

struct CommitPreview {
    theme: UiTheme,
    palette: DiffPalette,
    inline: InlineColors,
}

impl CommitPreview {
    fn render(&self, page: &CommitPage) -> AnyElement {
        let colors = self.theme.colors;
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("commit-preview-message")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(rem(PREVIEW_PADDING_X))
                    .pt(rem(PREVIEW_PADDING_TOP))
                    .pb(rem(PREVIEW_GAP))
                    .flex()
                    .flex_col()
                    .gap(rem(PREVIEW_GAP))
                    .child(
                        div().flex().flex_col().gap_2().child(self.eyebrow(page)).child(
                            div()
                                .debug_selector(|| "commit-preview-title".to_owned())
                                .typography(Typography::Heading)
                                .text_color(colors.text)
                                .child(page.title.summary.clone()),
                        ),
                    )
                    .children(self.body(page)),
            )
            .child(self.facts(page))
            .into_any_element()
    }

    fn eyebrow(&self, page: &CommitPage) -> Div {
        let colors = self.theme.colors;
        let (label, tone) = match page.title.kind {
            _ if page.is_working_tree() => ("Uncommitted", colors.text_secondary),
            Some(kind) => (kind.label(), kind_color(Some(kind), &colors)),
            None => ("Commit", colors.text_secondary),
        };
        let scope =
            page.title.scope.clone().map(|scope| {
                div().flex().items_center().gap_1p5().text_color(colors.text_muted).child("·").child(scope)
            });
        div()
            .debug_selector(|| "commit-preview-eyebrow".to_owned())
            .flex()
            .items_center()
            .gap_1p5()
            .typography(Typography::Small)
            .child(commit_marker(page, &colors).dot(&colors))
            .child(div().font_weight(FontWeight::MEDIUM).text_color(tone).child(label))
            .children(scope)
    }

    fn body(&self, page: &CommitPage) -> Option<Div> {
        if page.body.is_empty() {
            return None;
        }
        let document = MarkdownDocument::parse(page.body.as_str());
        Some(
            div()
                .debug_selector(|| "commit-preview-body".to_owned())
                .flex()
                .flex_col()
                .gap_2()
                .typography(Typography::Body)
                .text_color(self.theme.colors.text_secondary)
                .children(
                    document
                        .blocks()
                        .iter()
                        .enumerate()
                        .map(|(index, block)| self.block(index, block, document.source())),
                ),
        )
    }

    fn block(&self, index: usize, block: &MarkdownBlock, source: &str) -> AnyElement {
        match &block.kind {
            MarkdownBlockKind::Paragraph { content } | MarkdownBlockKind::Heading { content, .. } => {
                inline_text(("commit-body", index), content, self.inline)
            }
            MarkdownBlockKind::List { ordered, start, items } => div()
                .id(("commit-body", index))
                .flex()
                .flex_col()
                .gap_1()
                .children(items.iter().enumerate().map(|(position, item)| {
                    let marker = if *ordered {
                        format!("{}.", start.unwrap_or(1).saturating_add(position as u64))
                    } else {
                        "•".to_owned()
                    };
                    div()
                        .flex()
                        .gap_2()
                        .child(div().flex_none().text_color(self.theme.colors.text_muted).child(marker))
                        .child(div().flex_1().min_w_0().child(inline_text(
                            ("commit-body-item", position),
                            &item.content,
                            self.inline,
                        )))
                }))
                .into_any_element(),
            _ => div().child(source[block.source.bytes.clone()].trim().to_owned()).into_any_element(),
        }
    }

    fn facts(&self, page: &CommitPage) -> Div {
        let colors = self.theme.colors;
        let fact = |icon: IconName, text: SharedString| {
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(Icon::new(icon, colors.text_muted).size(tokens::ICON_SIZE_SMALL))
                .child(text)
        };
        let commit = match &page.short_id {
            Some(short_id) => fact(IconName::GitCommit, short_id.clone().into()).font_family(CODE_FONT),
            None => fact(IconName::GitCommit, "Working tree".into()),
        };
        div()
            .debug_selector(|| "commit-preview-facts".to_owned())
            .flex_none()
            .mx(rem(PREVIEW_PADDING_X))
            .pt(rem(PREVIEW_GAP))
            .pb(rem(PREVIEW_PADDING_BOTTOM))
            .border_t_1()
            .border_color(colors.border_subtle)
            .flex()
            .flex_col()
            .gap(rem(FACTS_GAP))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x_3()
                    .gap_y_1()
                    .typography(Typography::Small)
                    .text_color(colors.text_secondary)
                    .child(commit)
                    .children(
                        page.title
                            .pull_request
                            .map(|number| fact(IconName::GitPullRequest, format!("#{number}").into())),
                    )
                    .children(page.stat.map(|stat| fact(IconName::File, count(stat.files, "file").into())))
                    .children(page.stat.map(|stat| {
                        div().flex().gap_1p5().children(stat_badges(&self.palette, stat.added, stat.deleted))
                    }))
                    .child(fact(IconName::MessageCircle, comments_label(page.comments).into())),
            )
            .children(page.stat.and_then(|stat| change_bar(stat, &self.palette)))
    }
}

fn change_bar(stat: LineStat, palette: &DiffPalette) -> Option<Div> {
    let total = stat.added + stat.deleted;
    (total > 0).then(|| {
        let added = u16::try_from(stat.added.saturating_mul(1000) / total).unwrap_or(1000);
        div()
            .debug_selector(|| "commit-preview-change-bar".to_owned())
            .h(rem(CHANGE_BAR_HEIGHT))
            .w_full()
            .flex()
            .gap(rem(CHANGE_BAR_GAP))
            .rounded_full()
            .overflow_hidden()
            .when(stat.added > 0, |bar| {
                bar.child(div().h_full().w(relative(f32::from(added) / 1000.0)).bg(color(palette.addition)))
            })
            .when(stat.deleted > 0, |bar| bar.child(div().h_full().flex_1().bg(color(palette.deletion))))
    })
}

fn count(value: usize, noun: &str) -> String {
    if value == 1 { format!("1 {noun}") } else { format!("{value} {noun}s") }
}

fn comments_label(comments: usize) -> String {
    if comments == 0 { "No comments".to_owned() } else { count(comments, "comment") }
}

fn since_epoch() -> Duration {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default()
}
