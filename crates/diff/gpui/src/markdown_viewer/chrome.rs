use super::MarkdownReviewer;
use crate::{
    shortcuts::{ShortcutHelp, Shortcuts},
    ui::prelude::{
        Backdrop, Button, ButtonVariant, CODE_FONT, ControlSize, IconButton, IconName, PanelSide, ResizablePanel,
        ThemePicker, TreeRow, Typography, TypographyExt, rem,
    },
};
use clankerdiff_core::ReviewCommand;
use clankerdiff_markdown::{MarkdownFocusPane, MarkdownReviewCommand};
use gpui::{Context, Entity, Rems, div, prelude::*};
use std::path::Path;

const MIN_OUTLINE_WIDTH: f32 = 160.0;
const MAX_OUTLINE_WIDTH: f32 = 420.0;
const HEADER_HEIGHT: f32 = 56.0;

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

impl MarkdownReviewer {
    pub(super) fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.ui;
        let colors = theme.colors;
        let count = self.review().len();
        let (label, command) = if count == 0 {
            ("Approve".to_owned(), MarkdownReviewCommand::Approve)
        } else {
            (format!("Send feedback ({count})"), MarkdownReviewCommand::RequestChanges)
        };
        div()
            .debug_selector(|| "markdown-header".to_owned())
            .h(rem(HEADER_HEIGHT))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .border_b_1()
            .border_color(colors.border)
            .when(self.outline_available(), |header| {
                let open = self.outline_visible();
                header.child(
                    PanelSide::Left
                        .toggle_button("markdown-outline-toggle", open, "outline", theme)
                        .selected(open)
                        .on_click(cx.listener(|reviewer, _, _, cx| reviewer.toggle_outline(cx))),
                )
            })
            .child(self.render_title())
            .child(
                IconButton::new("markdown-shortcuts", IconName::Info, "Keyboard shortcuts", theme)
                    .on_click(Self::dispatch(ReviewCommand::ShowHelp, cx)),
            )
            .when(self.options.themes, |header| {
                header.child(
                    Button::new("markdown-theme", "Theme", theme)
                        .size(ControlSize::Small)
                        .disabled(!self.command_enabled(&ReviewCommand::OpenThemePicker.into()))
                        .on_click(Self::dispatch(ReviewCommand::OpenThemePicker, cx)),
                )
            })
            .when(self.capabilities.clipboard && count > 0, |header| {
                header.child(IconButton::new("markdown-copy", IconName::Copy, "Copy review", theme).on_click(
                    cx.listener(|reviewer, _, window, cx| {
                        reviewer.copy_review(window, cx);
                    }),
                ))
            })
            .child(
                Button::new("markdown-submit", label, theme)
                    .variant(ButtonVariant::Primary)
                    .size(ControlSize::Small)
                    .disabled(!self.command_enabled(&command))
                    .on_click(Self::dispatch(command, cx)),
            )
    }

    fn render_title(&self) -> impl IntoElement {
        let colors = self.ui.colors;
        let document = self.document();
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
        div()
            .flex_1()
            .min_w_0()
            .px_1()
            .flex()
            .flex_col()
            .child(
                div()
                    .debug_selector(|| "markdown-title".to_owned())
                    .typography(Typography::Title)
                    .text_color(colors.text)
                    .truncate()
                    .child(title),
            )
            .children(path.map(|path| {
                div()
                    .debug_selector(|| "markdown-path".to_owned())
                    .font_family(CODE_FONT)
                    .typography(Typography::Caption)
                    .text_color(colors.text_muted)
                    .truncate()
                    .child(path)
            }))
    }

    pub(super) fn render_outline(&self, cx: &mut Context<Self>) -> ResizablePanel {
        let theme = self.ui;
        let colors = theme.colors;
        let rows = self.render_outline_rows(cx);
        let viewer = cx.entity().downgrade();
        let reset = viewer.clone();
        ResizablePanel::new("markdown-outline", PanelSide::Left, self.outline_width, theme)
            .min_width(rem(MIN_OUTLINE_WIDTH))
            .max_width(rem(MAX_OUTLINE_WIDTH))
            .on_resize(move |width, _, cx| {
                let _ = viewer.update(cx, |reviewer, cx| reviewer.set_outline_width(width, cx));
            })
            .on_reset(move |_, cx| {
                let _ = reset.update(cx, |reviewer, cx| {
                    reviewer.set_outline_width(reviewer.options.outline_width, cx);
                });
            })
            .bg(colors.surface)
            .when(self.pane == MarkdownFocusPane::Outline, |panel| panel.border_color(colors.accent))
            .child(
                div()
                    .px_3()
                    .pt_4()
                    .pb_2()
                    .typography(Typography::Label)
                    .text_color(colors.text_muted)
                    .child("Contents"),
            )
            .child(
                div()
                    .id("markdown-outline-rows")
                    .track_scroll(&self.outline_scroll)
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_1()
                    .pb_2()
                    .flex()
                    .flex_col()
                    .children(rows),
            )
    }

    fn render_outline_rows(&self, cx: &mut Context<Self>) -> Vec<TreeRow> {
        let theme = self.ui;
        let colors = theme.colors;
        let active = self.active_heading();
        self.document()
            .outline()
            .iter()
            .enumerate()
            .map(|(index, heading)| {
                let target = heading.target_id;
                let comments = self.section_comment_count(index);
                TreeRow::new(("markdown-outline", index), heading.title.clone(), theme)
                    .depth(usize::from(heading.level.saturating_sub(1)))
                    .selected(active == Some(index))
                    .label_color(if heading.level <= 1 { colors.text } else { colors.text_secondary })
                    .on_click(cx.listener(move |reviewer, _, window, cx| {
                        reviewer.select(target, MarkdownFocusPane::Outline, window, cx);
                    }))
                    .when(comments > 0, |row| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .px_1p5()
                                .rounded_full()
                                .bg(colors.surface_selected)
                                .typography(Typography::Caption)
                                .text_color(colors.accent)
                                .child(comments.to_string()),
                        )
                    })
            })
            .collect()
    }

    fn section_comment_count(&self, index: usize) -> usize {
        let document = self.document();
        let outline = document.outline();
        let Some(heading) = outline.get(index) else {
            return 0;
        };
        let end = outline.get(index + 1).map_or(usize::MAX, |next| next.target_id.index());
        document
            .targets()
            .iter()
            .filter(|target| (heading.target_id.index()..end).contains(&target.id.index()))
            .map(|target| self.review().comments_for_target(document, target.id).len())
            .sum()
    }

    pub(super) fn render_shortcuts(&self) -> ShortcutHelp {
        ShortcutHelp::new(&self.ui)
            .section("Navigation", NAVIGATION)
            .section("Review", REVIEW)
            .section("Comment editor", DRAFT)
    }

    pub(super) fn render_theme_picker(&self, picker: &Entity<ThemePicker>) -> impl IntoElement {
        let dismissed = picker.clone();
        Backdrop::new("markdown-theme-picker-backdrop", self.ui)
            .on_dismiss(move |_, cx| dismissed.update(cx, ThemePicker::cancel))
            .child(picker.clone())
    }

    fn set_outline_width(&mut self, width: Rems, cx: &mut Context<Self>) {
        self.outline_width = width;
        cx.notify();
    }
}
