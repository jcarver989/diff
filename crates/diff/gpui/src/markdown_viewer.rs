#![allow(missing_docs)] // GPUI's `actions!` macro cannot attach per-action rustdoc.

mod chrome;
mod commands;
pub(crate) mod content;
mod document;
pub(crate) mod inline;
pub(crate) mod rows;

use crate::{
    ThemeChanged,
    comment_editor::{CommentEditor, CommentEditorEvent, init_input},
    ui::prelude::{SearchPicker, ThemePicker, ThemePickerEvent, Typography, TypographyExt, UI_FONT, UiTheme, rem},
};
use clankerdiff_core::{InteractionPhase, ReviewCapabilities, ReviewCommand};
use clankerdiff_markdown::{
    MarkdownDocument, MarkdownFocusPane, MarkdownReview, MarkdownReviewCommand, MarkdownReviewDecision,
    MarkdownReviewEvent, MarkdownReviewSession, MarkdownTargetId,
};
use clankerdiff_syntax::SyntaxHighlighter;
use clankerdiff_theme::{ReviewTheme, ThemeChoice};
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding, KeyContext, Rems, ScrollHandle,
    Subscription, Window, actions, div, prelude::*,
};
use inline::InlineColors;
use rows::{DocumentRow, document_rows};
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
        MarkdownApprove,
        MarkdownRequestChanges,
        MarkdownShowThemePicker,
        MarkdownShowShortcuts,
        MarkdownToggleOutline,
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
    pub outline_width: Rems,
    pub show_outline: bool,
    pub themes: bool,
}

impl Default for MarkdownReviewerOptions {
    fn default() -> Self {
        Self { outline_width: rem(220.0), show_outline: true, themes: true }
    }
}

#[derive(Default)]
enum Overlay {
    #[default]
    None,
    Help,
    Themes {
        picker: Entity<ThemePicker>,
        _subscription: Subscription,
    },
}

/// Shared GPUI rendered-Markdown review entity used by desktop and web hosts.
pub struct MarkdownReviewer {
    session: MarkdownReviewSession,
    capabilities: ReviewCapabilities,
    pane: MarkdownFocusPane,
    overlay: Overlay,
    outline_collapsed: bool,
    outline_width: Rems,
    document_scroll: ScrollHandle,
    outline_scroll: ScrollHandle,
    theme: ReviewTheme,
    ui: UiTheme,
    inline: InlineColors,
    highlighter: RefCell<SyntaxHighlighter>,
    rows: Vec<DocumentRow>,
    target_rows: HashMap<MarkdownTargetId, usize>,
    options: MarkdownReviewerOptions,
    editor: Option<Entity<CommentEditor>>,
    editor_subscription: Option<Subscription>,
    focus_handle: Option<FocusHandle>,
}

impl MarkdownReviewer {
    #[must_use]
    pub fn new(document: Arc<MarkdownDocument>) -> Self {
        Self::with_options(document, ReviewTheme::default(), MarkdownReviewerOptions::default())
    }

    #[must_use]
    pub fn with_options(document: Arc<MarkdownDocument>, theme: ReviewTheme, options: MarkdownReviewerOptions) -> Self {
        let (rows, target_rows) = layout_rows(&document);
        let ui = UiTheme::new(&theme);
        Self {
            session: MarkdownReviewSession::new(document),
            capabilities: ReviewCapabilities::default(),
            pane: MarkdownFocusPane::Document,
            overlay: Overlay::None,
            outline_collapsed: false,
            outline_width: options.outline_width,
            document_scroll: ScrollHandle::new(),
            outline_scroll: ScrollHandle::new(),
            inline: InlineColors::new(&theme, &ui),
            ui,
            theme,
            highlighter: RefCell::new(SyntaxHighlighter::default()),
            rows,
            target_rows,
            options,
            editor: None,
            editor_subscription: None,
            focus_handle: None,
        }
    }

    pub fn bind_keys(cx: &mut App) {
        const ANY: &str = "MarkdownReviewer";
        const BROWSE: &str = "MarkdownReviewer && mode == browse";
        const DRAFT: &str = "MarkdownReviewer && mode == draft";
        const HELP: &str = "MarkdownReviewer && mode == help";
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
            KeyBinding::new(
                "tab",
                MarkdownToggleFocus,
                Some("MarkdownReviewer && mode == browse && !CommentDismissControl"),
            ),
            KeyBinding::new(
                "enter",
                MarkdownOpenSelected,
                Some("MarkdownReviewer && mode == browse && !CommentDismissControl"),
            ),
            KeyBinding::new("y", MarkdownCopyReview, Some(BROWSE)),
            KeyBinding::new("cmd-shift-c", MarkdownCopyReview, Some(BROWSE)),
            KeyBinding::new("n", MarkdownNextHeading, Some(BROWSE)),
            KeyBinding::new("p", MarkdownPreviousHeading, Some(BROWSE)),
            KeyBinding::new("b", MarkdownToggleOutline, Some(BROWSE)),
            KeyBinding::new("c", MarkdownAddComment, Some(BROWSE)),
            KeyBinding::new("e", MarkdownEditComment, Some(BROWSE)),
            KeyBinding::new("x", MarkdownDeleteComment, Some(BROWSE)),
            KeyBinding::new("u", MarkdownUndoComment, Some(BROWSE)),
            KeyBinding::new("a", MarkdownApprove, Some(BROWSE)),
            KeyBinding::new("r", MarkdownRequestChanges, Some(BROWSE)),
            KeyBinding::new("shift-/", MarkdownShowShortcuts, Some(BROWSE)),
            KeyBinding::new("shift-/", MarkdownCancel, Some(HELP)),
            KeyBinding::new("t", MarkdownShowThemePicker, Some(BROWSE)),
            KeyBinding::new("cmd-shift-t", MarkdownShowThemePicker, Some(BROWSE)),
            KeyBinding::new("ctrl-shift-t", MarkdownShowThemePicker, Some(BROWSE)),
            KeyBinding::new("escape", MarkdownCancel, Some(ANY)),
            KeyBinding::new("cmd-enter", MarkdownSubmitComment, Some(DRAFT)),
            KeyBinding::new("ctrl-enter", MarkdownSubmitComment, Some(DRAFT)),
        ]);
        SearchPicker::bind_keys(cx);
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
        (self.rows, self.target_rows) = layout_rows(&document);
        self.session.replace_document(document);
        self.close_editor();
        cx.notify();
    }

    /// Returns the current theme.
    #[must_use]
    pub const fn theme(&self) -> &ReviewTheme {
        &self.theme
    }

    pub fn set_theme(&mut self, theme: ReviewTheme, cx: &mut Context<Self>) {
        if let Overlay::Themes { .. } = self.overlay {
            self.overlay = Overlay::None;
        }
        self.apply_theme(theme, cx);
    }

    fn open_theme_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        init_input(cx);
        let (theme, ui) = (self.theme.clone(), self.ui);
        let picker =
            cx.new(|cx| ThemePicker::new("markdown-theme-picker", &theme, ThemeChoice::catalog(), ui, window, cx));
        let subscription = cx.subscribe(&picker, |reviewer, _, event: &ThemePickerEvent, cx| {
            reviewer.on_theme_picker_event(event, cx);
        });
        self.overlay = Overlay::Themes { picker, _subscription: subscription };
        cx.notify();
    }

    fn on_theme_picker_event(&mut self, event: &ThemePickerEvent, cx: &mut Context<Self>) {
        match event {
            ThemePickerEvent::Preview(theme) => self.apply_theme(theme.clone(), cx),
            ThemePickerEvent::Commit(theme) => {
                self.overlay = Overlay::None;
                self.apply_theme(theme.clone(), cx);
                cx.emit(ThemeChanged { id: theme.id().to_string() });
            }
            ThemePickerEvent::Cancel(theme) => {
                self.overlay = Overlay::None;
                self.apply_theme(theme.clone(), cx);
            }
        }
    }

    fn update_theme_picker(
        &self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut ThemePicker, &mut Context<ThemePicker>),
    ) -> bool {
        let Overlay::Themes { picker, .. } = &self.overlay else {
            return false;
        };
        picker.update(cx, update);
        true
    }

    pub fn clear_review(&mut self, cx: &mut Context<Self>) {
        self.session.clear_review();
        self.close_editor();
        cx.notify();
    }

    fn apply_theme(&mut self, theme: ReviewTheme, cx: &mut Context<Self>) {
        self.ui = UiTheme::new(&theme);
        self.inline = InlineColors::new(&theme, &self.ui);
        self.theme = theme.clone();
        if let Some(editor) = &self.editor {
            editor.update(cx, |editor, cx| editor.set_theme(theme, cx));
        }
        cx.notify();
    }

    fn outline_available(&self) -> bool {
        self.options.show_outline && self.document().outline().len() > 1
    }

    fn outline_visible(&self) -> bool {
        self.outline_available() && !self.outline_collapsed
    }

    fn toggle_outline(&mut self, cx: &mut Context<Self>) {
        if !self.outline_available() {
            return;
        }
        self.outline_collapsed = !self.outline_collapsed;
        if self.outline_collapsed {
            self.pane = MarkdownFocusPane::Document;
        }
        cx.notify();
    }

    fn mount_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let body = self.session.draft().map_or_else(String::new, |draft| draft.body().to_owned());
        let editor = cx.new(|cx| CommentEditor::new(body, self.theme.clone(), window, cx));
        self.editor_subscription =
            Some(cx.subscribe_in(&editor, window, |reviewer, _, event: &CommentEditorEvent, window, cx| match event {
                CommentEditorEvent::Changed(body) => {
                    if let Some(draft) = reviewer.session.draft_mut() {
                        draft.set_body(body);
                    }
                    cx.notify();
                }
                CommentEditorEvent::Submit => {
                    reviewer.run(ReviewCommand::SubmitComment, window, cx);
                }
                CommentEditorEvent::Cancel => reviewer.run(ReviewCommand::Cancel, window, cx),
            }));
        self.editor = Some(editor.clone());
        editor.focus_handle(cx).focus(window, cx);
        self.reveal_selected_target();
        cx.notify();
    }

    fn close_editor(&mut self) {
        self.editor = None;
        self.editor_subscription = None;
    }

    fn discard_comment(&mut self, cx: &mut Context<Self>) {
        self.session.cancel_draft();
        self.close_editor();
        cx.notify();
    }

    fn dismiss_comment(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.draft().is_some_and(|draft| draft.editing() == Some(id)) {
            self.discard_comment(cx);
        }
        self.session.review_mut().remove_comment(id);
        if let Some(editor) = &self.editor {
            editor.focus_handle(cx).focus(window, cx);
        } else if let Some(focus) = &self.focus_handle {
            focus.focus(window, cx);
        }
        cx.notify();
    }

    fn copy_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let decision = if self.review().is_empty() {
            MarkdownReviewDecision::Approved
        } else {
            MarkdownReviewDecision::ChangesRequested
        };
        self.run(MarkdownReviewCommand::CopyReview(decision), window, cx);
    }

    fn key_context(&self) -> KeyContext {
        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("MarkdownReviewer");
        key_context.set(
            "mode",
            match self.command_context().phase {
                InteractionPhase::ThemePicker => "themes",
                InteractionPhase::Help => "help",
                InteractionPhase::Draft => "draft",
                InteractionPhase::Browse | InteractionPhase::CommitPicker | InteractionPhase::RepositoryPrompt => {
                    "browse"
                }
            },
        );
        key_context
    }
}

impl EventEmitter<MarkdownReviewEvent> for MarkdownReviewer {}
impl EventEmitter<ThemeChanged> for MarkdownReviewer {}

impl Focusable for MarkdownReviewer {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.focus_handle.clone().unwrap_or_else(|| cx.focus_handle())
    }
}

impl Render for MarkdownReviewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use MarkdownReviewCommand as C;
        use ReviewCommand as R;
        let focus = self.focus_handle.get_or_insert_with(|| cx.focus_handle()).clone();
        if window.focused(cx).is_none() && self.editor.is_none() {
            focus.focus(window, cx);
        }
        let palette = &self.theme.diff;

        div()
            .key_context(self.key_context())
            .track_focus(&focus)
            .on_action(Self::dispatch::<MarkdownNextTarget>(C::MoveSelection(1), cx))
            .on_action(Self::dispatch::<MarkdownPreviousTarget>(C::MoveSelection(-1), cx))
            .on_action(Self::dispatch::<MarkdownFirstTarget>(C::First, cx))
            .on_action(Self::dispatch::<MarkdownLastTarget>(C::Last, cx))
            .on_action(Self::dispatch::<MarkdownNextHeading>(C::NextHeading, cx))
            .on_action(Self::dispatch::<MarkdownPreviousHeading>(C::PreviousHeading, cx))
            .on_action(Self::dispatch::<MarkdownPageUp>(C::Page(-1), cx))
            .on_action(Self::dispatch::<MarkdownPageDown>(C::Page(1), cx))
            .on_action(Self::dispatch::<MarkdownToggleFocus>(C::ToggleFocus, cx))
            .on_action(Self::dispatch::<MarkdownOpenSelected>(C::OpenSelected, cx))
            .on_action(Self::dispatch::<MarkdownApprove>(C::Approve, cx))
            .on_action(Self::dispatch::<MarkdownRequestChanges>(C::RequestChanges, cx))
            .on_action(Self::dispatch::<MarkdownAddComment>(R::BeginComment, cx))
            .on_action(Self::dispatch::<MarkdownEditComment>(R::EditComment, cx))
            .on_action(Self::dispatch::<MarkdownDeleteComment>(R::DeleteComment, cx))
            .on_action(Self::dispatch::<MarkdownUndoComment>(R::UndoComment, cx))
            .on_action(Self::dispatch::<MarkdownSubmitComment>(R::SubmitComment, cx))
            .on_action(Self::dispatch::<MarkdownShowShortcuts>(R::ShowHelp, cx))
            .on_action(Self::dispatch::<MarkdownShowThemePicker>(R::OpenThemePicker, cx))
            .on_action(Self::dispatch::<MarkdownCancel>(R::Cancel, cx))
            .on_action(cx.listener(|reviewer, _: &MarkdownCopyReview, window, cx| {
                reviewer.copy_review(window, cx);
            }))
            .on_action(cx.listener(|reviewer, _: &MarkdownToggleOutline, _, cx| {
                reviewer.toggle_outline(cx);
            }))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .font_family(UI_FONT)
            .typography(Typography::Body)
            .bg(crate::style::color(palette.background))
            .text_color(crate::style::color(palette.foreground))
            .child(self.render_header(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .when(self.outline_visible(), |body| body.child(self.render_outline(cx)))
                    .child(self.render_document(cx)),
            )
            .map(|reviewer| match &self.overlay {
                Overlay::None => reviewer,
                Overlay::Help => reviewer.child(self.render_shortcuts()),
                Overlay::Themes { picker, .. } => reviewer.child(self.render_theme_picker(picker)),
            })
    }
}

fn layout_rows(document: &MarkdownDocument) -> (Vec<DocumentRow>, HashMap<MarkdownTargetId, usize>) {
    let rows = document_rows(document.blocks());
    let target_rows =
        rows.iter().enumerate().filter_map(|(index, row)| row.target.map(|target| (target, index))).collect();
    (rows, target_rows)
}
