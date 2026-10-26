use crate::{
    components::ListRow,
    icon::{Icon, IconName},
    theme::UiTheme,
    tokens::{self, rem},
};
use gpui::{Anchor, App, IntoElement, MouseButton, SharedString, Window, div, prelude::*};
use gpui_base::{Popover, Selectable};

type MenuAction = Box<dyn Fn(&mut Window, &mut App) + 'static>;

pub struct MenuItem(Entry);

enum Entry {
    Action {
        selector: SharedString,
        label: SharedString,
        icon: Option<IconName>,
        destructive: bool,
        action: MenuAction,
    },
    Separator,
}

impl MenuItem {
    #[must_use]
    pub fn new(
        selector: impl Into<SharedString>,
        label: impl Into<SharedString>,
        action: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self(Entry::Action {
            selector: selector.into(),
            label: label.into(),
            icon: None,
            destructive: false,
            action: Box::new(action),
        })
    }

    #[must_use]
    pub const fn separator() -> Self {
        Self(Entry::Separator)
    }

    #[must_use]
    pub fn icon(mut self, name: IconName) -> Self {
        if let Entry::Action { icon, .. } = &mut self.0 {
            *icon = Some(name);
        }
        self
    }

    #[must_use]
    pub fn destructive(mut self) -> Self {
        if let Entry::Action { destructive, .. } = &mut self.0 {
            *destructive = true;
        }
        self
    }
}

/// Wraps `trigger`, tagged `selector`, so that clicking it with `button` opens
/// the entries `items` builds in a menu below it, tagged `<selector>-menu`.
/// Choosing an entry closes the menu, then runs the entry's action.
///
/// `items` runs only when the menu opens, so that a view rendering many
/// closed menus builds none of their entries.
pub fn with_menu<T: Selectable + IntoElement + 'static>(
    selector: &str,
    trigger: T,
    button: MouseButton,
    items: impl FnOnce(&mut Window, &mut App) -> Vec<MenuItem> + 'static,
    theme: &UiTheme,
) -> Popover {
    let theme = *theme;
    let menu_selector = format!("{selector}-menu");
    Popover::new(SharedString::from(format!("{selector}-popover")))
        .mouse_button(button)
        .anchor(Anchor::TopLeft)
        .trigger(trigger)
        .content(move |_, window, cx| {
            let popover = cx.entity();
            let colors = theme.colors;
            let entries = items(window, cx).into_iter().map(move |MenuItem(entry)| {
                let Entry::Action { selector, label, icon, destructive, action } = entry else {
                    return div().my_1().mx_1().h_px().bg(colors.border).into_any_element();
                };
                let popover = popover.clone();
                let tint = destructive.then_some(colors.destructive);
                ListRow::new(selector, theme)
                    .on_click(move |_, window, cx| {
                        popover.update(cx, |popover, cx| popover.dismiss(window, cx));
                        action(window, cx);
                    })
                    .children(icon.map(|icon| Icon::new(icon, tint.unwrap_or(colors.text_muted))))
                    .child(div().min_w_0().truncate().when_some(tint, gpui::Styled::text_color).child(label))
                    .into_any_element()
            });
            div()
                .debug_selector(move || menu_selector)
                .w(rem(tokens::MENU_WIDTH))
                .p_1()
                .flex()
                .flex_col()
                .rounded(rem(tokens::RADIUS_CONTAINER))
                .border_1()
                .border_color(colors.border)
                .bg(colors.surface)
                .shadow_lg()
                .children(entries)
        })
}
