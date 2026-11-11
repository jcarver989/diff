use super::MarkdownHandle;
use crate::ui::{
    prelude::*,
    tokens::{ROW_HEIGHT, scaled},
};
use clankerdiff_core::ReviewCommand;
use clankerdiff_markdown::{MarkdownFocusPane, MarkdownReviewCommand};
use rouge_core::{RowKey, Styled, View, div, dynamic, text, virtual_list};
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

pub(super) fn header(handle: &MarkdownHandle, theme: UiTheme) -> impl View {
    let leading = dynamic({
        let handle = handle.clone();
        move || {
            let mut leading = div().flex_1().min_w(0.0).flex().row().items_center().gap(rem(8.0));
            if handle.outline_available() {
                let open = !handle.outline_collapsed.get();
                let toggle = handle.clone();
                leading = leading.child(
                    PanelSide::Left
                        .toggle_button("markdown-outline-toggle", open, "outline", theme)
                        .on_click(move || toggle.toggle_outline()),
                );
            }
            leading.child(title(&handle, theme))
        }
    });
    let trailing = dynamic({
        let handle = handle.clone();
        move || {
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
            let run = |command: MarkdownReviewCommand| {
                let handle = handle.clone();
                move || handle.run(command)
            };
            let mut trailing = div()
                .flex()
                .flex_none()
                .row()
                .items_center()
                .gap(rem(8.0))
                .child(
                    IconButton::new("markdown-shortcuts", IconName::Info, "Keyboard shortcuts", theme)
                        .on_click(run(ReviewCommand::ShowHelp.into())),
                )
                .child(
                    Button::new("markdown-theme", "Theme", theme)
                        .size(ControlSize::Small)
                        .disabled(!handle.command_enabled(ReviewCommand::OpenThemePicker.into()))
                        .on_click(run(ReviewCommand::OpenThemePicker.into())),
                );
            if caps.clipboard && count > 0 {
                let copy = handle.clone();
                trailing = trailing.child(
                    IconButton::new("markdown-copy", IconName::Copy, "Copy review", theme)
                        .on_click(move || copy.run(MarkdownReviewCommand::CopyReview(copy.copy_decision()))),
                );
            }
            trailing.child(
                Button::new("markdown-submit", label, theme)
                    .variant(ButtonVariant::Primary)
                    .size(ControlSize::Small)
                    .disabled(!handle.command_enabled(command))
                    .on_click(run(command)),
            )
        }
    });
    div()
        .test_id("markdown-header")
        .h(rem(HEADER_HEIGHT))
        .flex_none()
        .flex()
        .row()
        .items_center()
        .gap(rem(8.0))
        .px(rem(12.0))
        .border_b(1.0)
        .border_color(theme.paint(|c| c.border))
        .child(leading)
        .child(trailing)
}

fn title(handle: &MarkdownHandle, theme: UiTheme) -> impl View {
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
    let column = div().flex_1().min_w(0.0).px(rem(4.0)).flex().col().overflow_hidden().child(
        div()
            .test_id("markdown-title")
            .child(text(title).typography(Typography::Title).no_wrap().color(theme.paint(|c| c.text))),
    );
    match path {
        Some(path) => column.child(div().test_id("markdown-path").child(
            text(path).typography(Typography::Caption).font(CODE_FONT).no_wrap().color(theme.paint(|c| c.text_muted)),
        )),
        None => column,
    }
}

pub(super) fn outline(handle: &MarkdownHandle, theme: UiTheme) -> impl View {
    let rows = handle.rows;
    let revision = handle.revision;
    let pane = handle.pane;
    let list = virtual_list(move || rows.with(|rows| rows.document.outline().len()), scaled(ROW_HEIGHT), {
        let handle = handle.clone();
        move |index| div().px(rem(4.0)).child(outline_row(&handle, index, theme))
    })
    .row_identity(move |index| RowKey::new(index, revision.get()))
    .handle(handle.outline)
    .size_full();
    let width = handle.outline_width;
    ResizablePanel::new("markdown-outline", PanelSide::Left, width)
        .min_width(MIN_OUTLINE_WIDTH)
        .max_width(MAX_OUTLINE_WIDTH)
        .on_resize(move |value| width.set(value))
        .on_reset(move || width.set(super::DEFAULT_OUTLINE_WIDTH))
        .child(
            div()
                .size_full()
                .flex()
                .col()
                .overflow_hidden()
                .bg(theme.paint(|c| c.surface))
                .border_r(1.0)
                .border_color(
                    theme.paint(move |c| if pane.get() == MarkdownFocusPane::Outline { c.accent } else { c.border }),
                )
                .child(
                    div()
                        .px(rem(12.0))
                        .pt(rem(16.0))
                        .pb(rem(8.0))
                        .child(text("Contents").typography(Typography::Label).color(theme.paint(|c| c.text_muted))),
                )
                .child(div().flex_1().min_h(0.0).relative().pb(rem(8.0)).child(list).child(
                    div().absolute().right(0.0).top(0.0).bottom(0.0).child(ListScrollbar::list(handle.outline)),
                )),
        )
}

fn outline_row(handle: &MarkdownHandle, index: usize, theme: UiTheme) -> impl View {
    let Some(heading) = handle.with(|session| session.document().outline().get(index).cloned()) else {
        return div().into_any();
    };
    let selected = handle.selected;
    let outline = handle.clone();
    let annotations = handle.annotations;
    let counter = handle.clone();
    let primary = heading.level <= 1;
    let target = heading.target_id;
    TreeRow::new(format!("markdown-heading-{index}"), heading.title, theme)
        .depth(usize::from(heading.level.saturating_sub(1)))
        .label_color(theme.paint(move |c| if primary { c.text } else { c.text_secondary }))
        .selection(move || {
            selected.get();
            if outline.active_heading() == Some(index) { SelectionState::Selected } else { SelectionState::None }
        })
        .on_click({
            let handle = handle.clone();
            move || handle.select(target, MarkdownFocusPane::Outline)
        })
        .child(dynamic(move || {
            annotations.get();
            let count = counter.section_comment_count(index);
            if count == 0 {
                return div().into_any();
            }
            div()
                .flex_none()
                .px(rem(6.0))
                .rounded(scaled(ROW_HEIGHT))
                .bg(theme.paint(|c| c.surface_selected))
                .child(text(count.to_string()).typography(Typography::Caption).color(theme.paint(|c| c.accent)))
                .into_any()
        }))
        .into_any()
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

pub(super) fn shortcuts(theme: UiTheme, close: impl Fn() + 'static) -> impl View {
    let section = |title: &'static str, rows: Shortcuts| {
        div()
            .flex_1()
            .flex()
            .col()
            .gap(rem(4.0))
            .child(div().mb(rem(4.0)).child(text(title).typography(Typography::Label).color(theme.paint(|c| c.accent))))
            .children(rows.iter().map(move |(keys, description)| {
                div()
                    .flex()
                    .row()
                    .gap(rem(12.0))
                    .child(
                        div()
                            .w(rem(136.0))
                            .flex_none()
                            .child(text(*keys).typography(Typography::Code).no_wrap().color(theme.paint(|c| c.text))),
                    )
                    .child(
                        div().flex_1().min_w(0.0).child(
                            text(*description).typography(Typography::Body).color(theme.paint(|c| c.text_muted)),
                        ),
                    )
            }))
    };
    let sections = [("Navigation", NAVIGATION), ("Review", REVIEW), ("Comment editor", DRAFT)];
    sections.chunks(2).enumerate().fold(
        Modal::new("markdown-shortcuts-help", "Review shortcuts", theme)
            .hint("Esc to close")
            .size(ModalSize::Wide)
            .on_dismiss(close),
        |modal, (index, pair)| {
            let row = div().flex().row().gap(rem(20.0)).children(pair.iter().map(|(title, rows)| section(title, rows)));
            modal.child(if index > 0 { row.mt(rem(16.0)) } else { row })
        },
    )
}
