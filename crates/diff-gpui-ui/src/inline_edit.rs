use super::{
    theme::UiTheme,
    tokens::{RADIUS_CONTROL, rem},
};
use gpui::{
    App, AppContext, Entity, IntoElement, RenderOnce, SharedString, Window, div, prelude::*,
};
use gpui_base::input::{Enter, Escape, Input, InputState};

type EditHandler = Box<dyn Fn(&mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct InlineEdit {
    input: Entity<InputState>,
    theme: UiTheme,
    on_commit: Option<EditHandler>,
    on_cancel: Option<EditHandler>,
}

impl InlineEdit {
    pub fn input(
        value: impl Into<SharedString>,
        theme: UiTheme,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<InputState> {
        let input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).default_value(value);
            input.set_editor_style(theme.editor_style());
            input
        });
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        input
    }

    #[must_use]
    pub fn new(input: &Entity<InputState>, theme: UiTheme) -> Self {
        Self {
            input: input.clone(),
            theme,
            on_commit: None,
            on_cancel: None,
        }
    }

    #[must_use]
    pub fn on_commit(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_commit = Some(Box::new(handler));
        self
    }

    #[must_use]
    pub fn on_cancel(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_cancel = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for InlineEdit {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let colors = self.theme.colors;
        div()
            .w_full()
            .min_w_0()
            .px_1()
            .rounded(rem(RADIUS_CONTROL))
            .border_1()
            .border_color(colors.accent)
            .bg(colors.canvas)
            .when_some(self.on_commit, |edit, commit| {
                edit.on_action(move |_: &Enter, window, cx| commit(window, cx))
            })
            .when_some(self.on_cancel, |edit, cancel| {
                edit.on_action(move |_: &Escape, window, cx| cancel(window, cx))
            })
            .child(Input::new(&self.input))
    }
}
