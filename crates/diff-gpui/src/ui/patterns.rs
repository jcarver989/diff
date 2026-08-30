//! Product-level patterns assembled from design-system components.

use super::{
    components::{ListRow, Modal},
    theme::UiTheme,
    tokens,
};
use diff_theme::SelectionState;
use gpui::{
    App, ClickEvent, IntoElement, ParentElement, RenderOnce, SharedString, Window, div, prelude::*,
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// One selectable entry in a [`ThemePicker`].
pub struct ThemePickerItem {
    id: SharedString,
    name: SharedString,
    is_dark: bool,
    selected: bool,
    on_click: ClickHandler,
}

impl ThemePickerItem {
    #[must_use]
    pub fn new(
        id: impl Into<SharedString>,
        name: impl Into<SharedString>,
        is_dark: bool,
        selected: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            is_dark,
            selected,
            on_click: Box::new(on_click),
        }
    }
}

/// Shared theme-catalog modal used by every reviewer view.
#[derive(IntoElement)]
pub struct ThemePicker {
    id: SharedString,
    theme: UiTheme,
    width: f32,
    height: f32,
    items: Vec<ThemePickerItem>,
}

impl ThemePicker {
    #[must_use]
    pub fn new(
        id: impl Into<SharedString>,
        theme: UiTheme,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Self {
        Self {
            id: id.into(),
            theme,
            width: (viewport_width - tokens::MODAL_VIEWPORT_MARGIN)
                .clamp(1.0, tokens::PICKER_WIDTH),
            height: (viewport_height - 48.0).clamp(1.0, tokens::PICKER_HEIGHT),
            items: Vec::new(),
        }
    }

    #[must_use]
    pub fn items(mut self, items: impl IntoIterator<Item = ThemePickerItem>) -> Self {
        self.items.extend(items);
        self
    }
}

impl RenderOnce for ThemePicker {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let theme = self.theme;
        let id = self.id;
        let row_prefix = id.clone();
        let rows = self.items.into_iter().map(move |item| {
            ListRow::new(format!("{row_prefix}-{}", item.id), theme)
                .state(if item.selected {
                    SelectionState::Selected
                } else {
                    SelectionState::None
                })
                .on_click(item.on_click)
                .child(div().flex_1().child(item.name))
                .child(if item.is_dark { "Dark" } else { "Light" })
        });
        Modal::new(id.clone(), "Select theme", theme)
            .hint("Esc to close")
            .width(self.width)
            .height(self.height)
            .child(
                div()
                    .id(format!("{id}-list"))
                    .min_h_0()
                    .flex_1()
                    .overflow_y_scroll()
                    .children(rows),
            )
    }
}
