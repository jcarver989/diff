use crate::style;
use clankerdiff_theme::ReviewTheme;
use gpui::{
    App, Context, Entity, FocusHandle, Focusable, Global, MouseButton, SharedString, Subscription,
    Window, div, prelude::*, px,
};
use gpui_base::input::{Escape, InputEditorStyle, InputEvent, Textarea, TextareaState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommentEditorEvent {
    Changed(String),
    Submit,
    Cancel,
}

struct InputInitialized;

impl Global for InputInitialized {}

pub(crate) struct CommentEditor {
    input: Entity<TextareaState>,
    body: SharedString,
    theme: ReviewTheme,
    _subscriptions: Vec<Subscription>,
}

impl CommentEditor {
    pub(crate) fn new(
        body: String,
        theme: ReviewTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_placeholder(body, theme, "Leave a comment…", window, cx)
    }

    pub(crate) fn with_placeholder(
        body: String,
        theme: ReviewTheme,
        placeholder: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        if !cx.has_global::<InputInitialized>() {
            gpui_base::init(cx);
            cx.set_global(InputInitialized);
        }
        let input = cx.new(|cx| {
            let mut input = TextareaState::new(window, cx)
                .auto_grow(3, 12)
                .submit_on_enter(true)
                .placeholder(placeholder)
                .default_value(body.clone());
            input.set_editor_style(editor_style(&theme));
            input.set_selected_range(body.len()..body.len(), cx);
            input
        });
        let subscription = cx.subscribe(&input, |editor, input, event, cx| match event {
            InputEvent::Change => editor.sync_body(&input, cx),
            InputEvent::PressEnter { shift: false, .. } => cx.emit(CommentEditorEvent::Submit),
            InputEvent::Focus | InputEvent::Blur => cx.notify(),
            InputEvent::PressEnter { shift: true, .. } => {}
        });
        let observation = cx.observe(&input, |editor, input, cx| editor.sync_body(&input, cx));
        Self {
            input,
            body: body.into(),
            theme,
            _subscriptions: vec![subscription, observation],
        }
    }

    fn sync_body(&mut self, input: &Entity<TextareaState>, cx: &mut Context<Self>) {
        let body = input.read(cx).value();
        if self.body != body {
            self.body = body;
            cx.emit(CommentEditorEvent::Changed(self.body.to_string()));
            cx.notify();
        }
    }

    pub(crate) fn body(&self) -> &str {
        &self.body
    }

    pub(crate) fn is_blank(&self) -> bool {
        self.body.trim().is_empty()
    }

    pub(crate) fn set_theme(&mut self, theme: ReviewTheme, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.set_editor_style(editor_style(&theme));
            cx.notify();
        });
        self.theme = theme;
        cx.notify();
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn input(&self) -> Entity<TextareaState> {
        self.input.clone()
    }
}

fn editor_style(theme: &ReviewTheme) -> InputEditorStyle {
    let palette = &theme.diff;
    let mut selection = style::color(palette.accent);
    selection.a = 0.3;
    InputEditorStyle {
        foreground: style::color(palette.foreground),
        muted_foreground: style::color(palette.muted),
        background: style::color(palette.background),
        border: style::color(palette.border),
        selection,
        caret: style::color(palette.foreground),
        ..Default::default()
    }
}

impl Focusable for CommentEditor {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl gpui::EventEmitter<CommentEditorEvent> for CommentEditor {}

impl Render for CommentEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = &self.theme.diff;
        let focused = self.focus_handle(cx).is_focused(window);
        div()
            .id("comment-input")
            .debug_selector(|| "comment-input".to_owned())
            .min_h(px(96.0))
            .w_full()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(style::color(if focused {
                palette.accent
            } else {
                palette.border
            }))
            .bg(style::color(palette.background))
            .text_color(style::color(palette.foreground))
            .cursor_text()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|editor, _, window, cx| {
                    editor.input.update(cx, |input, cx| input.focus(window, cx));
                    cx.stop_propagation();
                }),
            )
            .on_action(cx.listener(|_, _: &Escape, _, cx| cx.emit(CommentEditorEvent::Cancel)))
            .child(Textarea::new(&self.input))
    }
}
