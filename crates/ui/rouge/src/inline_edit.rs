use crate::{
    fonts::UI_FONT,
    theme::UiTheme,
    tokens::{RADIUS_INNER, rem, scaled},
};
use rouge_core::{BuildCx, ElementId, Key, Styled, View, div, editable};
use rouge_reactive::{Signal, signal};

type Commit = Box<dyn Fn(&str)>;
pub struct InlineEdit {
    value: Signal<String>,
    theme: UiTheme,
    commit: Option<Commit>,
    cancel: Option<Box<dyn Fn()>>,
}
impl InlineEdit {
    pub fn input(value: impl Into<String>) -> Signal<String> {
        signal(value.into())
    }
    #[must_use]
    pub fn new(value: Signal<String>, theme: UiTheme) -> Self {
        Self { value, theme, commit: None, cancel: None }
    }
    #[must_use]
    pub fn on_commit(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.commit = Some(Box::new(f));
        self
    }
    #[must_use]
    pub fn on_cancel(mut self, f: impl Fn() + 'static) -> Self {
        self.cancel = Some(Box::new(f));
        self
    }
}
impl View for InlineEdit {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (theme, value) = (self.theme, self.value);
        div()
            .w_full()
            .min_w(0.0)
            .px(rem(4.0))
            .rounded(scaled(RADIUS_INNER))
            .border(1.0)
            .border_color(theme.paint(|c| c.accent))
            .bg(theme.paint(|c| c.canvas))
            .on_key_down(move |event| {
                if event.key == Key::Escape {
                    event.stop_propagation();
                    if let Some(f) = &self.cancel {
                        f();
                    }
                }
            })
            .child(
                editable(value)
                    .test_id("inline-edit")
                    .autofocus()
                    .select_all_on_focus()
                    .font(UI_FONT)
                    .text_size(rem(13.0))
                    .color(theme.paint(|c| c.text))
                    .w_full()
                    .on_submit(move |value| {
                        if let Some(f) = &self.commit {
                            f(value);
                        }
                    }),
            )
            .build(cx)
    }
}
