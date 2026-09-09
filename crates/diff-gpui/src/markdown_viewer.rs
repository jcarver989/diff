#![allow(missing_docs)] // GPUI's `actions!` macro cannot attach per-action rustdoc.

mod commands;

use crate::{
    DEFAULT_FONT_FAMILY, DEFAULT_FONT_SIZE, ThemeChanged,
    comment_editor::{CommentEditor, CommentEditorEvent},
    style,
    ui::{
        comments::{CommentCard, CommentComposer, CommentCount},
        prelude::{
            ActionBar, Button, ButtonVariant, ControlSize, ThemePicker, ThemePickerItem, UiTheme,
        },
    },
};
use clankerdiff_core::{ReviewCapabilities, ReviewCommand};
use clankerdiff_markdown::{
    MarkdownBlock, MarkdownBlockKind, MarkdownDocument, MarkdownFocusPane, MarkdownReview,
    MarkdownReviewCommand, MarkdownReviewDecision, MarkdownReviewEvent, MarkdownReviewSession,
    MarkdownTargetId, MarkdownTargetKind,
};
use clankerdiff_syntax::{LanguageHint, SyntaxHighlighter};
use clankerdiff_theme::ReviewTheme;
use clankerdiff_theme::ThemeSelection;
use gpui::{
    App, Context, Entity, EventEmitter, Focusable, HighlightStyle, KeyBinding, KeyContext,
    ScrollHandle, SharedString, StyledText, Subscription, Window, actions, div, prelude::*, px,
};
use std::{cell::RefCell, collections::HashMap, sync::Arc};

actions!(
    markdown_reviewer,
    [
        MarkdownNextTarget,
        MarkdownPreviousTarget,
        MarkdownFirstTarget,
        MarkdownLastTarget,
        MarkdownNextHeading,
        MarkdownPreviousHeading,
        MarkdownAddComment,
        MarkdownEditComment,
        MarkdownDeleteComment,
        MarkdownUndoComment,
        MarkdownSubmitComment,
        MarkdownCancelComment,
        MarkdownApprove,
        MarkdownRequestChanges,
        MarkdownShowThemePicker,
        MarkdownHideThemePicker,
        MarkdownNextTheme,
        MarkdownPreviousTheme,
        MarkdownCommitTheme,
        MarkdownPageUp,
        MarkdownPageDown,
        MarkdownToggleFocus,
        MarkdownOpenSelected,
        MarkdownCopyReview,
        MarkdownCancel
    ]
);

/// Renderer-specific Markdown reviewer options.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkdownReviewerOptions {
    pub font_size: f32,
    pub outline_width: f32,
    pub show_outline: bool,
}

impl Default for MarkdownReviewerOptions {
    fn default() -> Self {
        Self {
            font_size: DEFAULT_FONT_SIZE,
            outline_width: 260.0,
            show_outline: true,
        }
    }
}

#[derive(Debug, Clone)]
struct CodeInfo {
    info: Arc<str>,
    lines: Arc<[String]>,
    line_index: Option<usize>,
}

/// Shared GPUI rendered-Markdown review entity used by desktop and web hosts.
pub struct MarkdownReviewer {
    session: MarkdownReviewSession,
    capabilities: ReviewCapabilities,
    pane: MarkdownFocusPane,
    theme_selection: Option<ThemeSelection>,
    document_scroll: ScrollHandle,
    outline_scroll: ScrollHandle,
    theme: ReviewTheme,
    highlighter: RefCell<SyntaxHighlighter>,
    code_infos: HashMap<MarkdownTargetId, CodeInfo>,
    options: MarkdownReviewerOptions,
    font_size: f32,
    editor: Option<Entity<CommentEditor>>,
    editor_subscription: Option<Subscription>,
    focus_handle: Option<gpui::FocusHandle>,
}

impl MarkdownReviewer {
    #[must_use]
    pub fn new(document: Arc<MarkdownDocument>) -> Self {
        Self::with_options(
            document,
            ReviewTheme::default(),
            MarkdownReviewerOptions::default(),
        )
    }

    #[must_use]
    pub fn with_options(
        document: Arc<MarkdownDocument>,
        theme: ReviewTheme,
        options: MarkdownReviewerOptions,
    ) -> Self {
        Self {
            code_infos: code_infos(document.blocks()),
            session: MarkdownReviewSession::new(document),
            capabilities: ReviewCapabilities::default(),
            pane: MarkdownFocusPane::Document,
            theme_selection: None,
            document_scroll: ScrollHandle::new(),
            outline_scroll: ScrollHandle::new(),
            theme,
            highlighter: RefCell::new(SyntaxHighlighter::default()),
            options,
            font_size: options.font_size,
            editor: None,
            editor_subscription: None,
            focus_handle: None,
        }
    }

    pub fn bind_keys(cx: &mut App) {
        const BROWSE: &str = "MarkdownReviewer && mode == browse";
        const DRAFT: &str = "MarkdownReviewer && mode == draft";
        cx.bind_keys([
            KeyBinding::new("j", MarkdownNextTarget, Some(BROWSE)),
            KeyBinding::new("down", MarkdownNextTarget, Some(BROWSE)),
            KeyBinding::new("k", MarkdownPreviousTarget, Some(BROWSE)),
            KeyBinding::new("up", MarkdownPreviousTarget, Some(BROWSE)),
            KeyBinding::new("g", MarkdownFirstTarget, Some(BROWSE)),
            KeyBinding::new("home", MarkdownFirstTarget, Some(BROWSE)),
            KeyBinding::new("shift-g", MarkdownLastTarget, Some(BROWSE)),
            KeyBinding::new("end", MarkdownLastTarget, Some(BROWSE)),
            KeyBinding::new("pageup", MarkdownPageUp, Some(BROWSE)),
            KeyBinding::new("pagedown", MarkdownPageDown, Some(BROWSE)),
            KeyBinding::new("tab", MarkdownToggleFocus, Some(BROWSE)),
            KeyBinding::new("enter", MarkdownOpenSelected, Some(BROWSE)),
            KeyBinding::new("y", MarkdownCopyReview, Some(BROWSE)),
            KeyBinding::new("cmd-shift-c", MarkdownCopyReview, Some(BROWSE)),
            KeyBinding::new("n", MarkdownNextHeading, Some(BROWSE)),
            KeyBinding::new("p", MarkdownPreviousHeading, Some(BROWSE)),
            KeyBinding::new("c", MarkdownAddComment, Some(BROWSE)),
            KeyBinding::new("e", MarkdownEditComment, Some(BROWSE)),
            KeyBinding::new("x", MarkdownDeleteComment, Some(BROWSE)),
            KeyBinding::new("u", MarkdownUndoComment, Some(BROWSE)),
            KeyBinding::new("a", MarkdownApprove, Some(BROWSE)),
            KeyBinding::new("r", MarkdownRequestChanges, Some(BROWSE)),
            KeyBinding::new("t", MarkdownShowThemePicker, Some(BROWSE)),
            KeyBinding::new("cmd-shift-t", MarkdownShowThemePicker, Some(BROWSE)),
            KeyBinding::new("ctrl-shift-t", MarkdownShowThemePicker, Some(BROWSE)),
            KeyBinding::new(
                "escape",
                MarkdownHideThemePicker,
                Some("MarkdownReviewer && mode == themes"),
            ),
            KeyBinding::new(
                "down",
                MarkdownNextTheme,
                Some("MarkdownReviewer && mode == themes"),
            ),
            KeyBinding::new(
                "j",
                MarkdownNextTheme,
                Some("MarkdownReviewer && mode == themes"),
            ),
            KeyBinding::new(
                "up",
                MarkdownPreviousTheme,
                Some("MarkdownReviewer && mode == themes"),
            ),
            KeyBinding::new(
                "k",
                MarkdownPreviousTheme,
                Some("MarkdownReviewer && mode == themes"),
            ),
            KeyBinding::new(
                "enter",
                MarkdownCommitTheme,
                Some("MarkdownReviewer && mode == themes"),
            ),
            KeyBinding::new("escape", MarkdownCancel, Some(BROWSE)),
            KeyBinding::new("escape", MarkdownCancelComment, Some(DRAFT)),
            KeyBinding::new("cmd-enter", MarkdownSubmitComment, Some(DRAFT)),
            KeyBinding::new("ctrl-enter", MarkdownSubmitComment, Some(DRAFT)),
        ]);
    }

    #[must_use]
    pub const fn session(&self) -> &MarkdownReviewSession {
        &self.session
    }

    pub const fn session_mut(&mut self) -> &mut MarkdownReviewSession {
        &mut self.session
    }

    #[must_use]
    pub const fn document(&self) -> &Arc<MarkdownDocument> {
        self.session.document()
    }

    #[must_use]
    pub const fn review(&self) -> &MarkdownReview {
        self.session.review()
    }

    pub fn set_document(&mut self, document: Arc<MarkdownDocument>, cx: &mut Context<Self>) {
        self.code_infos = code_infos(document.blocks());
        self.session.replace_document(document);
        self.close_editor();
        cx.notify();
    }

    fn font_size(&self) -> f32 {
        self.font_size
    }

    /// Returns semantic component tokens for the current theme.
    fn ui_theme(&self) -> UiTheme {
        UiTheme::new(&self.theme)
    }

    pub fn set_theme(&mut self, theme: ReviewTheme, cx: &mut Context<Self>) {
        self.theme_selection = None;
        self.apply_theme(theme, cx);
    }

    fn apply_theme(&mut self, theme: ReviewTheme, cx: &mut Context<Self>) {
        self.theme = theme.clone();
        if let Some(editor) = &self.editor {
            editor.update(cx, |editor, cx| editor.set_theme(theme, cx));
        }
        cx.notify();
    }

    pub fn clear_review(&mut self, cx: &mut Context<Self>) {
        self.session.clear_review();
        self.close_editor();
        cx.notify();
    }

    fn open_editor(&mut self, editing: Option<u64>, window: &mut Window, cx: &mut Context<Self>) {
        let command = if editing.is_some() {
            ReviewCommand::EditComment
        } else {
            ReviewCommand::BeginComment
        };
        let _ = self.handle_command(command, window, cx);
    }

    fn mount_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let body = self
            .session
            .draft()
            .map_or_else(String::new, |draft| draft.body().to_owned());
        let editor = cx.new(|cx| CommentEditor::new(body, self.theme.clone(), cx));
        self.editor_subscription = Some(cx.subscribe_in(
            &editor,
            window,
            |reviewer, _, event: &CommentEditorEvent, window, cx| match event {
                CommentEditorEvent::Changed(body) => {
                    if let Some(draft) = reviewer.session.draft_mut() {
                        draft.set_body(body);
                    }
                    cx.notify();
                }
                CommentEditorEvent::Submit => {
                    let _ = reviewer.handle_command(ReviewCommand::SubmitComment, window, cx);
                }
                CommentEditorEvent::Cancel => {
                    let _ = reviewer.handle_command(ReviewCommand::Cancel, window, cx);
                }
            },
        ));
        self.editor = Some(editor.clone());
        editor.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn close_editor(&mut self) {
        self.editor = None;
        self.editor_subscription = None;
    }

    fn finish_comment(&mut self, cx: &mut Context<Self>) {
        self.session.submit_draft();
        self.close_editor();
        self.reveal_selected_target();
        cx.notify();
    }

    fn discard_comment(&mut self, cx: &mut Context<Self>) {
        self.session.cancel_draft();
        self.close_editor();
        cx.notify();
    }

    fn render_outline(&self, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let palette = &self.theme.diff;
        let headings = self.document().outline().to_vec();
        div()
            .id("markdown-outline-pane")
            .track_scroll(&self.outline_scroll)
            .w(px(self.options.outline_width))
            .h_full()
            .flex_shrink_0()
            .overflow_y_scroll()
            .border_r_1()
            .border_color(style::color(palette.border))
            .p_3()
            .child(
                div()
                    .mb_3()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child("Outline"),
            )
            .children(headings.into_iter().map(|heading| {
                let target = heading.target_id;
                let selected = self.session.selected_target() == Some(target);
                div()
                    .id(("markdown-outline", target.index()))
                    .pl(px(f32::from(heading.level.saturating_sub(1)) * 12.0))
                    .py_1()
                    .cursor_pointer()
                    .when(selected, |row| row.bg(style::color(palette.selection)))
                    .child(heading.title)
                    .on_click(cx.listener(move |reviewer, _, window, cx| {
                        let _ = reviewer.handle_command(
                            MarkdownReviewCommand::Focus(MarkdownFocusPane::Outline),
                            window,
                            cx,
                        );
                        let _ = reviewer.handle_command(
                            MarkdownReviewCommand::SelectTarget(target),
                            window,
                            cx,
                        );
                    }))
            }))
    }

    #[expect(
        clippy::too_many_lines,
        reason = "target content and its annotations form one GPUI element"
    )]
    fn render_target(
        &self,
        target_id: MarkdownTargetId,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let palette = &self.theme.diff;
        let Some(target) = self.document().target(target_id) else {
            return div().id(("missing-markdown-target", target_id.index()));
        };
        let selected = self.session.selected_target() == Some(target_id);
        let text = self
            .document()
            .rendered_target_text(target_id)
            .unwrap_or_default();
        let comments = self
            .review()
            .comments_for_target(self.document(), target_id)
            .collect::<Vec<_>>();
        let comment_count = comments.len();
        let is_code = matches!(
            target.kind,
            MarkdownTargetKind::CodeBlock | MarkdownTargetKind::CodeLine
        );
        let label = target.display_label.clone();
        let lines = target.source.lines;
        let editor = self
            .session
            .draft()
            .filter(|draft| draft.target() == target_id)
            .and(self.editor.clone());

        let rendered: SharedString = text.clone().into();
        let rendered = if let Some(info) = self.code_infos.get(&target_id) {
            let highlighted = self
                .highlighter
                .borrow_mut()
                .with_theme(&self.theme.syntax)
                .highlight_lines(
                    LanguageHint::InfoString(&info.info),
                    info.lines.iter().map(String::as_str),
                );
            let highlights = if let Some(index) = info.line_index {
                highlighted.get(index).cloned().unwrap_or_default()
            } else {
                flatten_line_spans(&info.lines, highlighted)
            };
            StyledText::new(rendered).with_highlights(highlights.iter().map(|span| {
                let style: HighlightStyle =
                    style::highlight_style(span.foreground, span.font_style);
                (span.range.clone(), style)
            }))
        } else {
            StyledText::new(rendered)
        };

        let content = div()
            .w_full()
            .flex()
            .items_start()
            .gap_3()
            .child(
                div()
                    .w(px(78.0))
                    .flex_shrink_0()
                    .text_color(style::color(palette.muted))
                    .child(if lines.start == lines.end {
                        format!("{}", lines.start)
                    } else {
                        format!("{}–{}", lines.start, lines.end)
                    }),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .when(is_code, |code| {
                        code.font_family(DEFAULT_FONT_FAMILY)
                            .bg(style::color(palette.selection))
                    })
                    .whitespace_normal()
                    .child(rendered),
            )
            .when(comment_count > 0, |row| {
                row.child(CommentCount::new(
                    comment_count,
                    self.font_size() - 2.0,
                    self.ui_theme(),
                ))
            });

        div()
            .id(("markdown-target", target_id.index()))
            .w_full()
            .p_3()
            .border_l_2()
            .border_color(style::color(if selected {
                palette.accent
            } else {
                palette.border
            }))
            .when(selected, |row| row.bg(style::color(palette.selection)))
            .cursor_pointer()
            .on_click(cx.listener(move |reviewer, _, window, cx| {
                let _ = reviewer.handle_command(
                    MarkdownReviewCommand::Focus(MarkdownFocusPane::Document),
                    window,
                    cx,
                );
                let _ = reviewer.handle_command(
                    MarkdownReviewCommand::SelectTarget(target_id),
                    window,
                    cx,
                );
            }))
            .child(
                div()
                    .text_size(px(self.font_size() - 3.0))
                    .text_color(style::color(palette.muted))
                    .child(label),
            )
            .child(content)
            .children(comments.into_iter().enumerate().map(|(index, comment)| {
                CommentCard::new(
                    comment.id,
                    if comment.outdated {
                        "Outdated comment"
                    } else {
                        "Comment"
                    },
                    comment.body.clone(),
                    self.font_size() - 3.0,
                    self.ui_theme(),
                    index + 1 == comment_count,
                )
            }))
            .children(editor.map(|editor| {
                let can_submit = !editor.read(cx).is_blank();
                let theme = self.ui_theme();
                let cancel = Button::new(
                    ("markdown-cancel-comment", target_id.index()),
                    "Cancel",
                    theme,
                )
                .size(ControlSize::Small)
                .on_click(cx.listener(|reviewer, _, window, cx| {
                    let _ = reviewer.handle_command(ReviewCommand::Cancel, window, cx);
                }));
                let submit = Button::new(
                    ("markdown-submit-comment", target_id.index()),
                    "Save comment",
                    theme,
                )
                .variant(ButtonVariant::Primary)
                .size(ControlSize::Small)
                .disabled(!can_submit)
                .on_click(cx.listener(|reviewer, _, window, cx| {
                    let _ = reviewer.handle_command(ReviewCommand::SubmitComment, window, cx);
                }));
                div().mt_2().child(CommentComposer::new(
                    editor,
                    "Review comment",
                    theme,
                    cancel,
                    submit,
                ))
            }))
    }

    fn render_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.review().len();
        let pane = match self.pane {
            MarkdownFocusPane::Document => "Document",
            MarkdownFocusPane::Outline => "Outline",
        };
        let theme = self.ui_theme();
        ActionBar::new(theme)
            .child(format!(
                "{pane} · Tab pane · PgUp/PgDn page · {count} comment(s) · c add · e edit · x delete · u undo · y copy"
            ))
            .child(div().flex_1())
            .child(
                Button::new("markdown-theme", "Theme", theme)
                    .disabled(!self.command_enabled(&ReviewCommand::OpenThemePicker.into()))
                    .size(ControlSize::Small)
                    .on_click(cx.listener(|reviewer, _, window, cx| {
                        let _ = reviewer.handle_command(ReviewCommand::OpenThemePicker, window, cx);
                    })),
            )
            .child(
                Button::new("markdown-request-changes", "Request changes", theme)
                    .disabled(!self.command_enabled(&MarkdownReviewCommand::RequestChanges))
                    .variant(ButtonVariant::Secondary)
                    .size(ControlSize::Small)
                    .on_click(cx.listener(|reviewer, _, window, cx| {
                        let _ = reviewer.handle_command(
                            MarkdownReviewCommand::RequestChanges,
                            window,
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("markdown-approve", "Approve", theme)
                    .disabled(!self.command_enabled(&MarkdownReviewCommand::Approve))
                    .variant(ButtonVariant::Primary)
                    .size(ControlSize::Small)
                    .on_click(cx.listener(|reviewer, _, window, cx| {
                        let _ = reviewer.handle_command(MarkdownReviewCommand::Approve, window, cx);
                    })),
            )
    }

    fn next_target(&mut self, _: &MarkdownNextTarget, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.handle_command(MarkdownReviewCommand::MoveSelection(1), window, cx);
    }
    fn previous_target(
        &mut self,
        _: &MarkdownPreviousTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.handle_command(MarkdownReviewCommand::MoveSelection(-1), window, cx);
    }
    fn first_target(
        &mut self,
        _: &MarkdownFirstTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.handle_command(MarkdownReviewCommand::First, window, cx);
    }
    fn last_target(&mut self, _: &MarkdownLastTarget, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.handle_command(MarkdownReviewCommand::Last, window, cx);
    }
    fn next_heading(
        &mut self,
        _: &MarkdownNextHeading,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.handle_command(MarkdownReviewCommand::NextHeading, window, cx);
    }
    fn previous_heading(
        &mut self,
        _: &MarkdownPreviousHeading,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.handle_command(MarkdownReviewCommand::PreviousHeading, window, cx);
    }
    fn add_comment(&mut self, _: &MarkdownAddComment, window: &mut Window, cx: &mut Context<Self>) {
        self.open_editor(None, window, cx);
    }
    fn edit_comment(
        &mut self,
        _: &MarkdownEditComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.handle_command(ReviewCommand::EditComment, window, cx);
    }
    fn delete_comment(
        &mut self,
        _: &MarkdownDeleteComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.handle_command(ReviewCommand::DeleteComment, window, cx);
    }
    fn undo_comment(
        &mut self,
        _: &MarkdownUndoComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.handle_command(ReviewCommand::UndoComment, window, cx);
    }
    fn submit_comment(
        &mut self,
        _: &MarkdownSubmitComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.handle_command(ReviewCommand::SubmitComment, window, cx);
    }
    fn cancel_comment(
        &mut self,
        _: &MarkdownCancelComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.handle_command(ReviewCommand::Cancel, window, cx);
    }
    fn approve(&mut self, _: &MarkdownApprove, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.handle_command(MarkdownReviewCommand::Approve, window, cx);
    }
    fn request_changes(
        &mut self,
        _: &MarkdownRequestChanges,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.handle_command(MarkdownReviewCommand::RequestChanges, window, cx);
    }

    fn show_theme_picker(
        &mut self,
        _: &MarkdownShowThemePicker,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.handle_command(ReviewCommand::OpenThemePicker, window, cx);
    }

    fn hide_theme_picker(
        &mut self,
        _: &MarkdownHideThemePicker,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.handle_command(ReviewCommand::Cancel, window, cx);
    }

    fn select_theme(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.theme_selection.as_ref().and_then(|selection| {
            selection
                .themes()
                .iter()
                .position(|choice| choice.theme.id().to_string() == id)
        });
        if let Some(index) = index {
            let _ = self.handle_command(ReviewCommand::SelectTheme(index), window, cx);
            let _ = self.handle_command(ReviewCommand::CommitTheme, window, cx);
        }
    }

    fn render_theme_picker(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.theme.id().to_string();
        let items = ReviewTheme::catalog().into_iter().map(|descriptor| {
            let id = descriptor.id.clone();
            ThemePickerItem::new(
                descriptor.id.clone(),
                descriptor.name,
                descriptor.is_dark,
                descriptor.id == current,
                cx.listener(move |reviewer, _, window, cx| reviewer.select_theme(&id, window, cx)),
            )
        });
        let viewport = window.viewport_size();
        ThemePicker::new(
            "markdown-theme-picker",
            self.ui_theme(),
            f32::from(viewport.width),
            f32::from(viewport.height),
        )
        .items(items)
    }

    fn next_theme(&mut self, _: &MarkdownNextTheme, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.handle_command(ReviewCommand::MoveTheme(1), window, cx);
    }

    fn previous_theme(
        &mut self,
        _: &MarkdownPreviousTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.handle_command(ReviewCommand::MoveTheme(-1), window, cx);
    }

    fn commit_theme(
        &mut self,
        _: &MarkdownCommitTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.handle_command(ReviewCommand::CommitTheme, window, cx);
    }

    fn page_up(&mut self, _: &MarkdownPageUp, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.handle_command(MarkdownReviewCommand::Page(-1), window, cx);
    }

    fn page_down(&mut self, _: &MarkdownPageDown, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.handle_command(MarkdownReviewCommand::Page(1), window, cx);
    }

    fn toggle_focus(
        &mut self,
        _: &MarkdownToggleFocus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.handle_command(MarkdownReviewCommand::ToggleFocus, window, cx);
    }

    fn open_selected(
        &mut self,
        _: &MarkdownOpenSelected,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.handle_command(MarkdownReviewCommand::OpenSelected, window, cx);
    }

    fn copy_review(&mut self, _: &MarkdownCopyReview, window: &mut Window, cx: &mut Context<Self>) {
        let decision = if self.review().is_empty() {
            MarkdownReviewDecision::Approved
        } else {
            MarkdownReviewDecision::ChangesRequested
        };
        let _ = self.handle_command(MarkdownReviewCommand::CopyReview(decision), window, cx);
    }

    fn cancel(&mut self, _: &MarkdownCancel, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.handle_command(ReviewCommand::Cancel, window, cx);
    }
}

fn flatten_line_spans(
    lines: &[String],
    highlighted: Vec<Vec<clankerdiff_theme::HighlightSpan>>,
) -> Vec<clankerdiff_theme::HighlightSpan> {
    let mut offset = 0;
    let mut flattened = Vec::new();
    for (line, spans) in lines.iter().zip(highlighted) {
        flattened.extend(spans.into_iter().map(|mut span| {
            span.range.start += offset;
            span.range.end += offset;
            span
        }));
        offset += line.len() + 1;
    }
    flattened
}

/// Collects the highlight info string for every code block, keyed by target.
fn code_infos(blocks: &[MarkdownBlock]) -> HashMap<MarkdownTargetId, CodeInfo> {
    fn collect(blocks: &[MarkdownBlock], infos: &mut HashMap<MarkdownTargetId, CodeInfo>) {
        for block in blocks {
            match &block.kind {
                MarkdownBlockKind::CodeBlock(code) => {
                    let info: Arc<str> = Arc::from(code.highlight_hint());
                    let lines: Arc<[String]> = Arc::from(
                        code.lines
                            .iter()
                            .map(|line| line.text.clone())
                            .collect::<Vec<_>>(),
                    );
                    if let Some(target) = code.target_id {
                        infos.insert(
                            target,
                            CodeInfo {
                                info: Arc::clone(&info),
                                lines: Arc::clone(&lines),
                                line_index: None,
                            },
                        );
                    }
                    for line in &code.lines {
                        if let Some(target) = line.target_id {
                            infos.insert(
                                target,
                                CodeInfo {
                                    info: Arc::clone(&info),
                                    lines: Arc::clone(&lines),
                                    line_index: Some(line.index),
                                },
                            );
                        }
                    }
                }
                MarkdownBlockKind::List { items, .. } => {
                    for item in items {
                        collect(&item.blocks, infos);
                    }
                }
                MarkdownBlockKind::BlockQuote { blocks } => collect(blocks, infos),
                MarkdownBlockKind::Heading { .. }
                | MarkdownBlockKind::Paragraph { .. }
                | MarkdownBlockKind::Table(_)
                | MarkdownBlockKind::HtmlFallback { .. }
                | MarkdownBlockKind::Rule => {}
            }
        }
    }
    let mut infos = HashMap::new();
    collect(blocks, &mut infos);
    infos
}

impl EventEmitter<MarkdownReviewEvent> for MarkdownReviewer {}
impl EventEmitter<ThemeChanged> for MarkdownReviewer {}

impl Focusable for MarkdownReviewer {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        self.focus_handle
            .clone()
            .unwrap_or_else(|| cx.focus_handle())
    }
}

impl Render for MarkdownReviewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self
            .focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        let palette = &self.theme.diff;
        let targets = self
            .document()
            .targets()
            .iter()
            .map(|target| target.id)
            .collect::<Vec<_>>();
        let mode = if self.theme_selection.is_some() {
            "themes"
        } else if self.editor.is_some() {
            "draft"
        } else {
            "browse"
        };
        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("MarkdownReviewer");
        key_context.set("mode", mode);

        div()
            .key_context(key_context)
            .track_focus(&focus)
            .on_action(cx.listener(Self::next_target))
            .on_action(cx.listener(Self::previous_target))
            .on_action(cx.listener(Self::first_target))
            .on_action(cx.listener(Self::last_target))
            .on_action(cx.listener(Self::next_heading))
            .on_action(cx.listener(Self::previous_heading))
            .on_action(cx.listener(Self::add_comment))
            .on_action(cx.listener(Self::edit_comment))
            .on_action(cx.listener(Self::delete_comment))
            .on_action(cx.listener(Self::undo_comment))
            .on_action(cx.listener(Self::submit_comment))
            .on_action(cx.listener(Self::cancel_comment))
            .on_action(cx.listener(Self::approve))
            .on_action(cx.listener(Self::request_changes))
            .on_action(cx.listener(Self::show_theme_picker))
            .on_action(cx.listener(Self::hide_theme_picker))
            .on_action(cx.listener(Self::next_theme))
            .on_action(cx.listener(Self::previous_theme))
            .on_action(cx.listener(Self::commit_theme))
            .on_action(cx.listener(Self::page_up))
            .on_action(cx.listener(Self::page_down))
            .on_action(cx.listener(Self::toggle_focus))
            .on_action(cx.listener(Self::open_selected))
            .on_action(cx.listener(Self::copy_review))
            .on_action(cx.listener(Self::cancel))
            .size_full()
            .flex()
            .flex_col()
            .font_family(DEFAULT_FONT_FAMILY)
            .text_size(px(self.font_size()))
            .bg(style::color(palette.background))
            .text_color(style::color(palette.foreground))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .when(self.options.show_outline, |root| {
                        root.child(self.render_outline(cx))
                    })
                    .child(
                        div()
                            .id("markdown-document")
                            .track_scroll(&self.document_scroll)
                            .flex_1()
                            .h_full()
                            .overflow_y_scroll()
                            .p_4()
                            .when(targets.is_empty(), |document| {
                                document.child(
                                    div()
                                        .p_4()
                                        .text_color(style::color(palette.muted))
                                        .child("Nothing to review"),
                                )
                            })
                            .children(
                                targets
                                    .into_iter()
                                    .map(|target| self.render_target(target, cx)),
                            ),
                    ),
            )
            .child(self.render_bar(cx))
            .when(self.theme_selection.is_some(), |reviewer| {
                reviewer.child(self.render_theme_picker(window, cx))
            })
    }
}
