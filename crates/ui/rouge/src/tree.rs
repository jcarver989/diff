use crate::{
    components::{SelectionState, list_row, pressable},
    icon::{Icon, IconName},
    theme::UiTheme,
    tokens::{ICON_SIZE, ICON_SIZE_SMALL, ROW_HEIGHT, TREE_INDENT},
    typography::UiStyles,
};
use rouge_core::{AnyView, Color, Div, Prop, Styled, component, div, text, view};
use rouge_reactive::Callback;

#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn indent(depth: usize) -> f32 {
    depth as f32 * TREE_INDENT
}

#[component]
pub fn tree_row(
    #[prop(into)] id: String,
    #[prop(into)] label: String,
    #[prop(default)] depth: usize,
    expanded: Option<bool>,
    icon: Option<IconName>,
    #[prop(reactive)] icon_color: Option<Prop<Color>>,
    #[prop(reactive)] label_color: Option<Prop<Color>>,
    leading: Option<AnyView>,
    #[prop(default)] state: SelectionState,
    #[prop(reactive)] selection: Option<Prop<SelectionState>>,
    #[prop(default, into)] on_click: Callback,
    #[prop(default, into)] on_toggle: Callback,
) -> Div {
    let theme = UiTheme::current();
    let slot = || view! { <div size=ICON_SIZE flex flex_none items_center justify_center/> };
    let caret = match expanded {
        Some(expanded) => {
            let icon = if expanded { IconName::ChevronDown } else { IconName::ChevronRight };
            let caret =
                slot().test_id(format!("{id}-toggle")).child(Icon::new(icon, theme.text_muted).size(ICON_SIZE_SMALL));
            pressable(caret, true, state != SelectionState::Disabled, on_toggle)
        }
        None => slot(),
    };
    let icon =
        icon.map(|icon| Icon::new(icon, move || icon_color.as_ref().map_or_else(|| theme.text_muted.get(), Prop::get)));
    let label_id = format!("{id}-label");
    view! {
        <list_row id depth state selection?=selection on_click h=ROW_HEIGHT>
            {caret}
            {leading}
            {icon}
            <div test_id=label_id flex_1 min_w=0.0 overflow_hidden>
                <text body no_wrap color={move || label_color.as_ref().map_or_else(|| theme.text.get(), Prop::get)}>
                    {label}
                </text>
            </div>
        </list_row>
    }
}
