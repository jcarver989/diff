use crate::{components::icon_button, icon::IconName};
use rouge_core::{AnyView, Styled, component, div, root_font_size};
use rouge_reactive::{Callback, Signal};
use rouge_widgets::{ResizablePanel, Side, resizable_panel as panel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelSide {
    Left,
    Right,
}

#[component]
pub fn panel_toggle(
    side: PanelSide,
    #[prop(into)] id: String,
    open: bool,
    name: &str,
    #[prop(default, into)] on_click: Callback,
) -> AnyView {
    let icon = match side {
        PanelSide::Left => IconName::PanelLeft,
        PanelSide::Right => IconName::PanelRight,
    };
    let label = format!("{} {name}", if open { "Hide" } else { "Show" });
    icon_button(id, icon, label, None, false, open, true, on_click)
}

#[component]
pub fn resizable_panel(
    #[prop(into)] id: String,
    side: PanelSide,
    width_in_rem: Signal<f32>,
    min_width: Option<f32>,
    max_width: Option<f32>,
    #[prop(default, into)] on_resize: Callback<(f32,)>,
    #[prop(default, into)] on_reset: Callback,
) -> ResizablePanel {
    let side = match side {
        PanelSide::Left => Side::Left,
        PanelSide::Right => Side::Right,
    };
    panel(side, width_in_rem)
        .unit(root_font_size())
        .min_size(min_width.unwrap_or(0.0))
        .max_size(max_width.unwrap_or(f32::MAX))
        .on_resize(move |width| on_resize.call(width))
        .on_reset(move || on_reset.call())
        .child(div().test_id(id).absolute().inset(0.0).pass_through())
}
