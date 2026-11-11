use crate::{
    components::{ListRow, SelectionState},
    icon::{Icon, IconName},
    theme::UiTheme,
    tokens::{ICON_SIZE, ICON_SIZE_SMALL, ROW_HEIGHT, TREE_INDENT, rem},
    typography::{Typography, TypographyExt},
};
use rouge_core::{AnyView, BuildCx, Color, ElementId, IntoProp, Prop, Styled, View, div, text};
use std::rc::Rc;

#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn indent(depth: usize) -> f32 {
    depth as f32 * TREE_INDENT
}
pub struct TreeRow {
    id: String,
    label: String,
    theme: UiTheme,
    depth: usize,
    expanded: Option<bool>,
    icon: Option<IconName>,
    icon_color: Option<Prop<Color>>,
    label_color: Option<Prop<Color>>,
    leading: Option<AnyView>,
    trailing: Vec<AnyView>,
    state: SelectionState,
    selection: Option<Rc<dyn Fn() -> SelectionState>>,
    click: Option<Rc<dyn Fn()>>,
    toggle: Option<Rc<dyn Fn()>>,
}
impl TreeRow {
    pub fn new(id: impl Into<String>, label: impl Into<String>, theme: UiTheme) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            theme,
            depth: 0,
            expanded: None,
            icon: None,
            icon_color: None,
            label_color: None,
            leading: None,
            trailing: Vec::new(),
            state: SelectionState::None,
            selection: None,
            click: None,
            toggle: None,
        }
    }
    #[must_use]
    pub fn depth(mut self, value: usize) -> Self {
        self.depth = value;
        self
    }
    #[must_use]
    pub fn expanded(mut self, value: bool) -> Self {
        self.expanded = Some(value);
        self
    }
    #[must_use]
    pub fn icon(mut self, value: IconName) -> Self {
        self.icon = Some(value);
        self
    }
    #[must_use]
    pub fn icon_color<T>(mut self, color: impl IntoProp<Color, T>) -> Self {
        self.icon_color = Some(color.into_prop());
        self
    }
    #[must_use]
    pub fn label_color<T>(mut self, color: impl IntoProp<Color, T>) -> Self {
        self.label_color = Some(color.into_prop());
        self
    }
    #[must_use]
    pub fn selection(mut self, selection: impl Fn() -> SelectionState + 'static) -> Self {
        self.selection = Some(Rc::new(selection));
        self
    }
    #[must_use]
    pub fn child(mut self, child: impl View) -> Self {
        self.trailing.push(child.into_any());
        self
    }
    #[must_use]
    pub fn leading(mut self, value: impl View) -> Self {
        self.leading = Some(value.into_any());
        self
    }
    #[must_use]
    pub fn state(mut self, state: SelectionState) -> Self {
        self.state = state;
        self
    }
    #[must_use]
    pub fn selected(self, selected: bool) -> Self {
        self.state(if selected { SelectionState::Selected } else { SelectionState::None })
    }
    #[must_use]
    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.click = Some(Rc::new(f));
        self
    }
    #[must_use]
    pub fn on_toggle(mut self, f: impl Fn() + 'static) -> Self {
        self.toggle = Some(Rc::new(f));
        self
    }
}
impl View for TreeRow {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let theme = self.theme;
        let mut row = ListRow::new(self.id.clone(), theme).depth(self.depth).state(self.state).h(rem(ROW_HEIGHT));
        if let Some(selection) = self.selection {
            row = row.selection(move || selection());
        }
        let slot = || div().w(rem(ICON_SIZE)).h(rem(ICON_SIZE)).flex().flex_none().items_center().justify_center();
        if let Some(expanded) = self.expanded {
            let caret = slot().test_id(format!("{}-toggle", self.id)).child(
                Icon::new(
                    if expanded { IconName::ChevronDown } else { IconName::ChevronRight },
                    theme.paint(|c| c.text_muted),
                )
                .size(ICON_SIZE_SMALL),
            );
            row = row.child(crate::components::activate(
                caret,
                true,
                self.state != SelectionState::Disabled,
                self.toggle,
            ));
        } else {
            row = row.child(slot());
        }
        if let Some(leading) = self.leading {
            row = row.child(leading);
        }
        if let Some(icon) = self.icon {
            let color = self.icon_color;
            row =
                row.child(Icon::new(icon, move || color.as_ref().map_or_else(|| theme.colors().text_muted, Prop::get)));
        }
        if let Some(click) = self.click {
            row = row.on_click(move || click());
        }
        let label_color = self.label_color;
        row = row.child(
            div().test_id(format!("{}-label", self.id)).flex_1().min_w(0.0).overflow_hidden().child(
                text(self.label)
                    .typography(Typography::Body)
                    .no_wrap()
                    .color(move || label_color.as_ref().map_or_else(|| theme.colors().text, Prop::get)),
            ),
        );
        for child in self.trailing {
            row = row.child(child);
        }
        row.build(cx)
    }
}
