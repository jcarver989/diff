use crate::{ReviewHandle, style};
use clankerdiff_core::{CommitKind, CommitPage, DiffReviewCommand, InteractionPhase, LineStat};
use clankerdiff_rouge_ui::{
    fonts::CODE_FONT,
    icon::{Icon, IconName},
    picker::{PickerItem, PickerMarker, SearchPicker, SearchPickerEvent},
    theme::{UiColors, UiTheme, color},
    tokens::{self, rem, scaled},
    typography::{Typography, TypographyExt},
};
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{AnyView, Color, Div, Styled, View, div, pct, text};
use rouge_reactive::ReadSignal;
use std::rc::Rc;
use web_time::{SystemTime, UNIX_EPOCH};

const PREVIEW_PADDING_X: f32 = 24.0;
const PREVIEW_PADDING_TOP: f32 = 24.0;
const PREVIEW_PADDING_BOTTOM: f32 = 20.0;
const PREVIEW_GAP: f32 = 14.0;
const FACTS_GAP: f32 = 10.0;
const CHANGE_BAR_HEIGHT: f32 = 4.0;
const CHANGE_BAR_GAP: f32 = 2.0;
const MARKER_SIZE: f32 = 7.0;

pub(crate) fn commit_picker(handle: &ReviewHandle, theme: UiTheme) -> SearchPicker {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let pages = Rc::new(handle.with(|state| state.session().commit_pages(now).unwrap_or_default()));
    let current = pages.iter().position(|page| page.current).unwrap_or(0);
    let colors = theme.colors();
    let items = pages
        .iter()
        .enumerate()
        .map(|(index, page)| {
            let item = PickerItem::new(format!("commit-page-{index}"), page.title.summary.clone())
                .marker(marker(page, &colors))
                .keywords(format!("{} {} {}", page.subject, page.id_label(), page.body))
                .checked(page.current);
            match &page.title.scope {
                Some(scope) => item.prefix(scope.clone()),
                None => item,
            }
        })
        .collect();
    let palette = handle.theme();
    let handle = handle.clone();
    SearchPicker::new("commit-picker", items, theme, move |event| match event {
        SearchPickerEvent::Chose(index) => {
            handle.handle_command(DiffReviewCommand::GoToPage(index));
        }
        SearchPickerEvent::Dismissed => handle.dismiss_picker(InteractionPhase::CommitPicker),
        _ => (),
    })
    .searchable("Search commits…")
    .width(rem(tokens::PICKER_PREVIEW_WIDTH))
    .height(rem(tokens::PICKER_PREVIEW_HEIGHT))
    .max_list_height(pct(100.0))
    .highlight(current)
    .preview(move |index| pages.get(index).map_or_else(|| div().into_any(), |page| preview(page, theme, palette)))
}

fn marker(page: &CommitPage, colors: &UiColors) -> PickerMarker {
    if page.is_working_tree() { PickerMarker::Pending } else { PickerMarker::Dot(kind_color(page.title.kind, colors)) }
}

const fn kind_color(kind: Option<CommitKind>, colors: &UiColors) -> Color {
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

fn preview(page: &CommitPage, theme: UiTheme, palette: ReadSignal<ReviewTheme>) -> AnyView {
    let mut message =
        div()
            .flex_1()
            .min_h(0.0)
            .px(rem(PREVIEW_PADDING_X))
            .pt(rem(PREVIEW_PADDING_TOP))
            .pb(rem(PREVIEW_GAP))
            .flex()
            .col()
            .gap(rem(PREVIEW_GAP))
            .child(div().flex().col().gap(rem(8.0)).child(eyebrow(page, theme)).child(
                div().test_id("commit-preview-title").child(
                    text(page.title.summary.clone()).typography(Typography::Heading).color(theme.paint(|c| c.text)),
                ),
            ));
    if !page.body.trim().is_empty() {
        message = message.child(rouge_core::scroll().test_id("commit-preview-body").flex_1().min_h(0.0).child(
            text(page.body.trim().to_owned()).typography(Typography::Body).color(theme.paint(|c| c.text_secondary)),
        ));
    }
    div()
        .test_id("commit-details")
        .size_full()
        .flex()
        .col()
        .child(message)
        .child(facts(page, theme, palette))
        .into_any()
}

fn eyebrow(page: &CommitPage, theme: UiTheme) -> Div {
    let colors = theme.colors();
    let (label, tone) = match page.title.kind {
        _ if page.is_working_tree() => ("Uncommitted", colors.text_secondary),
        Some(kind) => (kind.label(), kind_color(Some(kind), &colors)),
        None => ("Commit", colors.text_secondary),
    };
    let dot = match marker(page, &colors) {
        PickerMarker::Dot(dot) => div().w(rem(MARKER_SIZE)).h(rem(MARKER_SIZE)).rounded(scaled(MARKER_SIZE)).bg(dot),
        PickerMarker::Pending => div()
            .w(rem(MARKER_SIZE + 2.0))
            .h(rem(MARKER_SIZE + 2.0))
            .rounded(scaled(MARKER_SIZE))
            .border(1.0)
            .border_color(colors.text_muted),
    };
    let mut row = div()
        .test_id("commit-preview-eyebrow")
        .flex()
        .row()
        .items_center()
        .gap(rem(6.0))
        .child(dot)
        .child(text(label).typography(Typography::Small).weight(500.0).color(tone));
    if let Some(scope) = page.title.scope.clone() {
        row = row
            .child(text("·").typography(Typography::Small).color(colors.text_muted))
            .child(text(scope).typography(Typography::Small).color(colors.text_muted));
    }
    row
}

fn facts(page: &CommitPage, theme: UiTheme, palette: ReadSignal<ReviewTheme>) -> Div {
    let fact = move |icon: IconName, label: String, code: bool| {
        let label = text(label).typography(Typography::Small).color(theme.paint(|c| c.text_secondary)).no_wrap();
        div()
            .flex()
            .row()
            .items_center()
            .gap(rem(6.0))
            .child(Icon::new(icon, theme.paint(|c| c.text_muted)).size(tokens::ICON_SIZE_SMALL))
            .child(if code { label.font(CODE_FONT) } else { label })
    };
    let mut row = div().flex().row().flex_wrap().items_center().gap(rem(12.0)).child(match &page.short_id {
        Some(short_id) => fact(IconName::GitCommit, short_id.clone(), true),
        None => fact(IconName::GitCommit, "Working tree".to_owned(), false),
    });
    if let Some(number) = page.title.pull_request {
        row = row.child(fact(IconName::GitPullRequest, format!("#{number}"), false));
    }
    if let Some(stat) = page.stat {
        row =
            row.child(fact(IconName::File, count(stat.files, "file"), false)).child(style::stat_badges(stat, palette));
    }
    if let Some(age) = &page.age {
        row = row.child(fact(IconName::Timer, age.clone(), false));
    }
    row = row.child(fact(IconName::MessageCircle, comments_label(page.comments), false));
    let mut facts = div()
        .test_id("commit-preview-facts")
        .flex_none()
        .mx(rem(PREVIEW_PADDING_X))
        .pt(rem(PREVIEW_GAP))
        .pb(rem(PREVIEW_PADDING_BOTTOM))
        .border_t(1.0)
        .border_color(theme.paint(|c| c.border_subtle))
        .flex()
        .col()
        .gap(rem(FACTS_GAP))
        .child(row);
    if let Some(bar) = page.stat.and_then(|stat| change_bar(stat, palette)) {
        facts = facts.child(bar);
    }
    facts
}

#[expect(clippy::cast_precision_loss, reason = "Line counts only set a proportional bar width")]
fn change_bar(stat: LineStat, palette: ReadSignal<ReviewTheme>) -> Option<Div> {
    let total = stat.added + stat.deleted;
    (total > 0).then(|| {
        let mut bar = div()
            .test_id("commit-preview-change-bar")
            .h(rem(CHANGE_BAR_HEIGHT))
            .w_full()
            .flex()
            .row()
            .gap(rem(CHANGE_BAR_GAP))
            .rounded(scaled(CHANGE_BAR_HEIGHT))
            .overflow_hidden();
        if stat.added > 0 {
            bar = bar.child(
                div()
                    .h_full()
                    .w(pct(100.0 * stat.added as f32 / total as f32))
                    .bg(move || palette.with(|t| color(t.diff.addition))),
            );
        }
        if stat.deleted > 0 {
            bar = bar.child(div().h_full().flex_1().bg(move || palette.with(|t| color(t.diff.deletion))));
        }
        bar
    })
}

fn count(value: usize, noun: &str) -> String {
    if value == 1 { format!("1 {noun}") } else { format!("{value} {noun}s") }
}

fn comments_label(comments: usize) -> String {
    if comments == 0 { "No comments".to_owned() } else { count(comments, "comment") }
}
