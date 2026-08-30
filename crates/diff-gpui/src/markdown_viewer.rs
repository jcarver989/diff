#![allow(missing_docs)] // GPUI's `actions!` macro cannot attach per-action rustdoc.

use crate::{
    DEFAULT_FONT_FAMILY, ThemeChanged,
    comment_editor::{CommentEditor, CommentEditorEvent},
    style,
    ui::{
        comments::{CommentCard, CommentComposer, CommentCount},
        prelude::{
            ActionBar, Button, ButtonVariant, ControlSize, ThemePicker, ThemePickerItem, UiTheme,
        },
    },
};
use diff_markdown::{
    MarkdownBlock, MarkdownBlockKind, MarkdownDocument, MarkdownReview, MarkdownReviewEvent,
    MarkdownReviewSession, MarkdownTargetId, MarkdownTargetKind,
};
use diff_syntax::{LanguageHint, SyntaxHighlighter};
use diff_theme::DiffTheme;
use gpui::{
    App, Context, Entity, EventEmitter, Focusable, HighlightStyle, KeyBinding, KeyContext,
    SharedString, StyledText, Subscription, Window, actions, div, prelude::*, px,
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
            font_size: 16.0,
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
    theme: DiffTheme,
    highlighter: RefCell<SyntaxHighlighter>,
    code_infos: HashMap<MarkdownTargetId, CodeInfo>,
    options: MarkdownReviewerOptions,
    editor: Option<Entity<CommentEditor>>,
    editor_subscription: Option<Subscription>,
    theme_picker_open: bool,
    focus_handle: Option<gpui::FocusHandle>,
}

impl MarkdownReviewer {
    #[must_use]
    pub fn new(document: Arc<MarkdownDocument>) -> Self {
        Self::with_options(
            document,
            DiffTheme::default(),
            MarkdownReviewerOptions::default(),
        )
    }

    #[must_use]
    pub fn with_options(
        document: Arc<MarkdownDocument>,
        theme: DiffTheme,
        options: MarkdownReviewerOptions,
    ) -> Self {
        Self {
            code_infos: code_infos(document.blocks()),
            session: MarkdownReviewSession::new(document),
            theme,
            highlighter: RefCell::new(SyntaxHighlighter::default()),
            options,
            editor: None,
            editor_subscription: None,
            theme_picker_open: false,
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

    /// Returns semantic component tokens for the current theme.
    fn ui_theme(&self) -> UiTheme {
        UiTheme::new(&self.theme)
    }

    pub fn set_theme(&mut self, theme: DiffTheme, cx: &mut Context<Self>) {
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

    fn select(&mut self, target: MarkdownTargetId, cx: &mut Context<Self>) {
        if self.session.select_target(target) {
            self.close_editor();
            cx.notify();
        }
    }

    fn open_editor(&mut self, editing: Option<u64>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session.begin_draft(editing) {
            return;
        }
        let body = self
            .session
            .draft()
            .map_or_else(String::new, |draft| draft.body().to_owned());
        let editor = cx.new(|cx| CommentEditor::new(body, self.theme.clone(), cx));
        self.editor_subscription = Some(cx.subscribe(
            &editor,
            |reviewer, _, event: &CommentEditorEvent, cx| match event {
                CommentEditorEvent::Changed(body) => {
                    if let Some(draft) = reviewer.session.draft_mut() {
                        draft.set_body(body);
                    }
                    cx.notify();
                }
                CommentEditorEvent::Submit => reviewer.finish_comment(cx),
                CommentEditorEvent::Cancel => reviewer.discard_comment(cx),
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
        if let Some(editor) = &self.editor
            && let Some(draft) = self.session.draft_mut()
        {
            draft.set_body(editor.read(cx).body());
        }
        self.session.submit_draft();
        self.close_editor();
        cx.notify();
    }

    fn discard_comment(&mut self, cx: &mut Context<Self>) {
        self.session.cancel_draft();
        self.close_editor();
        cx.notify();
    }

    fn move_target(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.session.move_target(delta);
        self.close_editor();
        cx.notify();
    }

    fn render_outline(&self, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let palette = self.theme.palette();
        let headings = self.document().outline().to_vec();
        div()
            .id("markdown-outline-pane")
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
                    .on_click(cx.listener(move |reviewer, _, _, cx| reviewer.select(target, cx)))
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
        let palette = self.theme.palette();
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
                .with_theme(&self.theme)
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
                    self.options.font_size - 2.0,
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
            .on_click(cx.listener(move |reviewer, _, _, cx| reviewer.select(target_id, cx)))
            .child(
                div()
                    .text_size(px(self.options.font_size - 3.0))
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
                    self.options.font_size - 3.0,
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
                .on_click(cx.listener(|reviewer, _, _, cx| reviewer.discard_comment(cx)));
                let submit = Button::new(
                    ("markdown-submit-comment", target_id.index()),
                    "Save comment",
                    theme,
                )
                .variant(ButtonVariant::Primary)
                .size(ControlSize::Small)
                .disabled(!can_submit)
                .on_click(cx.listener(|reviewer, _, _, cx| reviewer.finish_comment(cx)));
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
        let theme = self.ui_theme();
        ActionBar::new(theme)
            .child(format!(
                "{count} comment(s) · c add · e edit · x delete · u undo"
            ))
            .child(div().flex_1())
            .child(
                Button::new("markdown-theme", "Theme", theme)
                    .size(ControlSize::Small)
                    .on_click(cx.listener(|reviewer, _, window, cx| {
                        reviewer.show_theme_picker(&MarkdownShowThemePicker, window, cx);
                    })),
            )
            .child(
                Button::new("markdown-request-changes", "Request changes", theme)
                    .variant(ButtonVariant::Secondary)
                    .size(ControlSize::Small)
                    .on_click(cx.listener(|reviewer, _, _, cx| reviewer.emit_request_changes(cx))),
            )
            .child(
                Button::new("markdown-approve", "Approve", theme)
                    .variant(ButtonVariant::Primary)
                    .size(ControlSize::Small)
                    .on_click(cx.listener(|reviewer, _, _, cx| reviewer.emit_approve(cx))),
            )
    }

    fn emit_approve(&mut self, cx: &mut Context<Self>) {
        if let Ok(event) = self.session.approve() {
            cx.emit(event);
        }
    }

    fn emit_request_changes(&mut self, cx: &mut Context<Self>) {
        if let Ok(event) = self.session.request_changes() {
            cx.emit(event);
        }
    }

    fn next_target(&mut self, _: &MarkdownNextTarget, _: &mut Window, cx: &mut Context<Self>) {
        self.move_target(1, cx);
    }
    fn previous_target(
        &mut self,
        _: &MarkdownPreviousTarget,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_target(-1, cx);
    }
    fn first_target(&mut self, _: &MarkdownFirstTarget, _: &mut Window, cx: &mut Context<Self>) {
        self.session.select_first_target();
        cx.notify();
    }
    fn last_target(&mut self, _: &MarkdownLastTarget, _: &mut Window, cx: &mut Context<Self>) {
        self.session.select_last_target();
        cx.notify();
    }
    fn next_heading(&mut self, _: &MarkdownNextHeading, _: &mut Window, cx: &mut Context<Self>) {
        self.session.next_heading();
        cx.notify();
    }
    fn previous_heading(
        &mut self,
        _: &MarkdownPreviousHeading,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session.previous_heading();
        cx.notify();
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
        let id = self.session.comment_id_at_selection();
        self.open_editor(id, window, cx);
    }
    fn delete_comment(
        &mut self,
        _: &MarkdownDeleteComment,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.delete_comment_at_selection() {
            cx.notify();
        }
    }
    fn undo_comment(&mut self, _: &MarkdownUndoComment, _: &mut Window, cx: &mut Context<Self>) {
        if self.session.undo_last_comment() {
            cx.notify();
        }
    }
    fn submit_comment(
        &mut self,
        _: &MarkdownSubmitComment,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_comment(cx);
    }
    fn cancel_comment(
        &mut self,
        _: &MarkdownCancelComment,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.discard_comment(cx);
    }
    fn approve(&mut self, _: &MarkdownApprove, _: &mut Window, cx: &mut Context<Self>) {
        self.emit_approve(cx);
    }
    fn request_changes(
        &mut self,
        _: &MarkdownRequestChanges,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.emit_request_changes(cx);
    }

    fn show_theme_picker(
        &mut self,
        _: &MarkdownShowThemePicker,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.theme_picker_open = true;
        cx.notify();
    }

    fn hide_theme_picker(
        &mut self,
        _: &MarkdownHideThemePicker,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.theme_picker_open = false;
        cx.notify();
    }

    fn select_theme(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Ok(theme) = DiffTheme::builtin(id) {
            self.theme_picker_open = false;
            self.set_theme(theme, cx);
            cx.emit(ThemeChanged { id: id.to_owned() });
        }
    }

    fn render_theme_picker(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.theme.id().to_string();
        let items = DiffTheme::catalog().into_iter().map(|descriptor| {
            let id = descriptor.id.clone();
            ThemePickerItem::new(
                descriptor.id.clone(),
                descriptor.name,
                descriptor.is_dark,
                descriptor.id == current,
                cx.listener(move |reviewer, _, _, cx| reviewer.select_theme(&id, cx)),
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

    #[expect(
        clippy::unused_self,
        reason = "GPUI action handlers must take the entity as their receiver"
    )]
    fn cancel(&mut self, _: &MarkdownCancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(MarkdownReviewEvent::Cancel);
    }
}

fn flatten_line_spans(
    lines: &[String],
    highlighted: Vec<Vec<diff_theme::HighlightSpan>>,
) -> Vec<diff_theme::HighlightSpan> {
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
        let palette = self.theme.palette();
        let targets = self
            .document()
            .targets()
            .iter()
            .map(|target| target.id)
            .collect::<Vec<_>>();
        let mode = if self.theme_picker_open {
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
            .on_action(cx.listener(Self::cancel))
            .size_full()
            .flex()
            .flex_col()
            .font_family(DEFAULT_FONT_FAMILY)
            .text_size(px(self.options.font_size))
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
            .when(self.theme_picker_open, |reviewer| {
                reviewer.child(self.render_theme_picker(window, cx))
            })
    }
}
