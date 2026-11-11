use crate::{components::IconButton, icon::IconName, theme::UiTheme};
use rouge_core::{BuildCx, ElementId, View, root_font_size};
use rouge_reactive::Signal;
use rouge_widgets::{Side, resizable_panel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelSide {
    Left,
    Right,
}
impl PanelSide {
    pub fn toggle_button(self, id: impl Into<String>, open: bool, name: &str, theme: UiTheme) -> IconButton {
        IconButton::new(
            id,
            match self {
                Self::Left => IconName::PanelLeft,
                Self::Right => IconName::PanelRight,
            },
            format!("{} {name}", if open { "Hide" } else { "Show" }),
            theme,
        )
        .selected(open)
        .keyboard_focusable()
    }
}
pub struct ResizablePanel(rouge_widgets::ResizablePanel);
impl ResizablePanel {
    pub fn new(id: impl Into<String>, side: PanelSide, width_in_rem: Signal<f32>) -> Self {
        use rouge_core::{Styled, div};
        Self(
            resizable_panel(
                match side {
                    PanelSide::Left => Side::Left,
                    PanelSide::Right => Side::Right,
                },
                width_in_rem,
            )
            .unit(root_font_size())
            .child(div().test_id(id).absolute().inset(0.0).pass_through()),
        )
    }
    #[must_use]
    pub fn min_width(self, width: f32) -> Self {
        Self(self.0.min_size(width))
    }
    #[must_use]
    pub fn max_width(self, width: f32) -> Self {
        Self(self.0.max_size(width))
    }
    #[must_use]
    pub fn on_resize(self, f: impl Fn(f32) + 'static) -> Self {
        Self(self.0.on_resize(f))
    }
    #[must_use]
    pub fn on_reset(self, f: impl Fn() + 'static) -> Self {
        Self(self.0.on_reset(f))
    }
    #[must_use]
    pub fn child(self, child: impl View) -> Self {
        Self(self.0.child(child))
    }
}
impl View for ResizablePanel {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        self.0.build(cx)
    }
}
