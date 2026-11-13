use crate::{ReviewHandle, style};
use clankerdiff_core::{CommitKind, CommitPage, DiffReviewCommand, InteractionPhase, LineStat};
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{Color, Div, Length, Palette, Styled, Theme, View, div, pct, px, scroll, text, view};
use rouge_reactive::ReadSignal;
use rouge_widgets::{
    CODE_FONT, Icon, IconName, PickerItem, PickerMarker, SearchPickerEvent, SearchPickerHandle, UiStyles,
    search_picker, tokens,
};
use std::rc::Rc;
use web_time::{SystemTime, UNIX_EPOCH};

const PREVIEW_PADDING_X: f32 = 24.0;
const PREVIEW_PADDING_TOP: f32 = 24.0;
const PREVIEW_PADDING_BOTTOM: f32 = 20.0;
const PREVIEW_GAP: f32 = 14.0;
const FACTS_GAP: f32 = 10.0;
const CHANGE_BAR_HEIGHT: f32 = 4.0;
const CHANGE_BAR_GAP: f32 = 2.0;

pub(crate) fn commit_picker(handle: &ReviewHandle) -> (SearchPickerHandle, Div) {
    let handle = *handle;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let pages = Rc::new(handle.with(|state| state.session().commit_pages(now).unwrap_or_default()));
    let current = pages.iter().position(|page| page.current).unwrap_or(0);
    let colors = Theme::current().get();
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
    let picker = SearchPickerHandle::new(items, move |event| match event {
        SearchPickerEvent::Chose(index) => {
            handle.handle_command(DiffReviewCommand::GoToPage(index));
        }
        SearchPickerEvent::Dismissed => handle.dismiss_picker(InteractionPhase::CommitPicker),
        _ => (),
    })
    .highlighted(current);
    let commits = view! {
        <search_picker id="commit-picker" handle=picker searchable="Search commits…"
                       width=Length::design(tokens::PICKER_PREVIEW_WIDTH)
                       height=Length::design(tokens::PICKER_PREVIEW_HEIGHT) max_list_height=pct(100.0)
                       preview={move |index: usize| pages.get(index).map(|page| preview(page, palette)).into_any()}/>
    };
    (picker, commits)
}

fn marker(page: &CommitPage, colors: &Palette) -> PickerMarker {
    if page.is_working_tree() { PickerMarker::Pending } else { PickerMarker::Dot(kind_color(page.title.kind, colors)) }
}

const fn kind_color(kind: Option<CommitKind>, colors: &Palette) -> Color {
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

fn preview(page: &CommitPage, palette: ReadSignal<ReviewTheme>) -> Div {
    let message = page.body.trim();
    view! {
        <div test_id="commit-details" size_full flex col>
            <div flex_1 min_h=0.0 px=PREVIEW_PADDING_X pt=PREVIEW_PADDING_TOP pb=PREVIEW_GAP flex col gap=PREVIEW_GAP>
                <div flex col gap=8.0>
                    {eyebrow(page)}
                    <div test_id="commit-preview-title">
                        <text heading primary>{page.title.summary.clone()}</text>
                    </div>
                </div>
                {(!message.is_empty()).then(|| view! {
                    <scroll test_id="commit-preview-body" flex_1 min_h=0.0>
                        <text body secondary>{message.to_owned()}</text>
                    </scroll>
                })}
            </div>
            {facts(page, palette)}
        </div>
    }
}

fn eyebrow(page: &CommitPage) -> Div {
    let colors = Theme::current().get();
    let (label, tone) = match page.title.kind {
        _ if page.is_working_tree() => ("Uncommitted", colors.text_secondary),
        Some(kind) => (kind.label(), kind_color(Some(kind), &colors)),
        None => ("Commit", colors.text_secondary),
    };
    view! {
        <div test_id="commit-preview-eyebrow" flex row items_center gap=6.0>
            {marker(page, &colors)}
            <text small weight=500.0 color=tone>{label}</text>
            {page.title.scope.clone().map(|scope| view! {
                <text small color=colors.text_muted>"·"</text>
                <text small color=colors.text_muted>{scope}</text>
            })}
        </div>
    }
}

fn facts(page: &CommitPage, palette: ReadSignal<ReviewTheme>) -> Div {
    let theme = Theme::current();
    let fact = move |icon: IconName, label: String, code: bool| {
        view! {
            <div flex row items_center gap=6.0>
                {Icon::new(icon, theme.text_muted).size(tokens::ICON_SIZE_SMALL)}
                <text small secondary no_wrap when:font=(code, CODE_FONT)>{label}</text>
            </div>
        }
    };
    let commit = match &page.short_id {
        Some(short_id) => fact(IconName::GitCommit, short_id.clone(), true),
        None => fact(IconName::GitCommit, "Working tree".to_owned(), false),
    };
    view! {
        <div test_id="commit-preview-facts" flex_none mx=PREVIEW_PADDING_X pt=PREVIEW_GAP pb=PREVIEW_PADDING_BOTTOM
             border_t=px(1.0) border_color=theme.border_subtle flex col gap=FACTS_GAP>
            <div flex row flex_wrap items_center gap=12.0>
                {commit}
                {page.title.pull_request.map(|number| fact(IconName::GitPullRequest, format!("#{number}"), false))}
                {page.stat.map(|stat| view! {
                    {fact(IconName::File, count(stat.files, "file"), false)}
                    {style::stat_badges(stat, palette)}
                })}
                {page.age.as_ref().map(|age| fact(IconName::Timer, age.clone(), false))}
                {fact(IconName::MessageCircle, comments_label(page.comments), false)}
            </div>
            {page.stat.and_then(|stat| change_bar(stat, palette))}
        </div>
    }
}

#[expect(clippy::cast_precision_loss, reason = "Line counts only set a proportional bar width")]
fn change_bar(stat: LineStat, palette: ReadSignal<ReviewTheme>) -> Option<Div> {
    let total = stat.added + stat.deleted;
    (total > 0).then(|| {
        view! {
            <div test_id="commit-preview-change-bar" h=CHANGE_BAR_HEIGHT w_full flex row gap=CHANGE_BAR_GAP
                 rounded=CHANGE_BAR_HEIGHT overflow_hidden>
                {(stat.added > 0).then(|| view! {
                    <div h_full w=pct(100.0 * stat.added as f32 / total as f32)
                         bg={move || palette.with(|t| Color::from(t.diff.addition))}/>
                })}
                {(stat.deleted > 0).then(|| view! {
                    <div h_full flex_1 bg={move || palette.with(|t| Color::from(t.diff.deletion))}/>
                })}
            </div>
        }
    })
}

fn count(value: usize, noun: &str) -> String {
    if value == 1 { format!("1 {noun}") } else { format!("{value} {noun}s") }
}

fn comments_label(comments: usize) -> String {
    if comments == 0 { "No comments".to_owned() } else { count(comments, "comment") }
}
