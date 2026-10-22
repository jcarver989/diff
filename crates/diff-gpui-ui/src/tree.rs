use crate::{
    icon::{Icon, IconName},
    theme::UiTheme,
    tokens::{self, rem},
    typography::{Typography, TypographyExt},
};
use gpui::{
    AnyElement, App, ClickEvent, ElementId, Hsla, IntoElement, ParentElement, Rems, RenderOnce,
    Role, SharedString, Window, div, prelude::*,
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct TreeRow {
    id: ElementId,
    label: SharedString,
    depth: usize,
    expanded: Option<bool>,
    icon: Option<(IconName, Option<Hsla>)>,
    label_color: Option<Hsla>,
    aria_label: Option<SharedString>,
    selected: bool,
    theme: UiTheme,
    on_click: Option<ClickHandler>,
    children: Vec<AnyElement>,
}

impl TreeRow {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, theme: UiTheme) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            depth: 0,
            expanded: None,
            icon: None,
            label_color: None,
            aria_label: None,
            selected: false,
            theme,
            on_click: None,
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    #[must_use]
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }

    #[must_use]
    pub fn icon(mut self, icon: IconName, color: Option<Hsla>) -> Self {
        self.icon = Some((icon, color));
        self
    }

    #[must_use]
    pub fn label_color(mut self, color: Hsla) -> Self {
        self.label_color = Some(color);
        self
    }

    #[must_use]
    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.aria_label = Some(label.into());
        self
    }

    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    #[must_use]
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl ParentElement for TreeRow {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for TreeRow {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let colors = self.theme.colors;
        let selector = self.id.to_string();
        let chevron = self.expanded.map(|expanded| {
            if expanded {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            }
        });
        div()
            .id(self.id)
            .debug_selector(move || selector)
            .role(Role::TreeItem)
            .aria_label(self.aria_label.unwrap_or_else(|| self.label.clone()))
            .aria_level(self.depth + 1)
            .aria_selected(self.selected)
            .when_some(
                self.expanded,
                gpui::StatefulInteractiveElement::aria_expanded,
            )
            .w_full()
            .min_w_0()
            .h(rem(tokens::ROW_HEIGHT))
            .flex_none()
            .pl(indent(self.depth))
            .pr_2()
            .flex()
            .items_center()
            .gap_1()
            .rounded(rem(tokens::RADIUS_CONTROL))
            .typography(Typography::Body)
            .text_color(self.label_color.unwrap_or(colors.text))
            .when(self.selected, |row| row.bg(colors.surface_selected))
            .when(!self.selected, |row| {
                row.hover(move |row| row.bg(colors.surface_hover))
            })
            .when_some(self.on_click, |row, handler| {
                row.cursor_pointer().on_click(handler)
            })
            .child(
                div()
                    .flex_none()
                    .size(rem(tokens::ICON_SIZE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .children(chevron.map(|chevron| {
                        Icon::new(chevron, colors.text_muted).size(tokens::ICON_SIZE_SMALL)
                    })),
            )
            .children(
                self.icon
                    .map(|(icon, color)| Icon::new(icon, color.unwrap_or(colors.text_muted))),
            )
            .child(div().flex_1().min_w_0().ml_1().truncate().child(self.label))
            .children(self.children)
    }
}

/// The leading padding of a row nested `depth` levels deep.
pub(crate) fn indent(depth: usize) -> Rems {
    let depth = f32::from(u16::try_from(depth).unwrap_or(u16::MAX));
    rem(tokens::CONTROL_PADDING_X_SMALL + depth * tokens::TREE_INDENT)
}
