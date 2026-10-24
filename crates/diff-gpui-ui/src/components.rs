//! Reusable, stateless GPUI components.

use super::{
    icon::{Icon, IconName},
    theme::{UiStyle, UiTheme},
    tokens::{self, rem},
    tree::indent,
    typography::{Typography, TypographyExt},
};
pub use clankerdiff_theme::{
    ButtonVariant, ControlSize, ControlState, InteractionState, ModalSize, NoticeTone,
    SelectionState,
};
use gpui::{
    AnyElement, AnyView, App, ClickEvent, Context, Div, ElementId, IntoElement, ParentElement,
    Refineable, Rems, Render, RenderOnce, Role, SharedString, Stateful, StyleRefinement, Window,
    div, prelude::*,
};
use gpui_base::Selectable;

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
type DismissHandler = Box<dyn Fn(&mut Window, &mut App) + 'static>;

/// Applies a resolved semantic style and the shared hover and click
/// affordances of interactive components.
fn interactive(
    element: Stateful<Div>,
    style: UiStyle,
    hover: UiStyle,
    disabled: bool,
    on_click: Option<ClickHandler>,
) -> Stateful<Div> {
    element
        .text_color(style.foreground)
        .when_some(style.background, gpui::Styled::bg)
        .when(style.emphasized, |element| {
            element.font_weight(gpui::FontWeight::SEMIBOLD)
        })
        .when(!disabled, |element| {
            element.cursor_pointer().when(hover != style, |element| {
                element.hover(move |element| {
                    let element = element.text_color(hover.foreground);
                    if let Some(background) = hover.background {
                        element.bg(background)
                    } else {
                        element
                    }
                })
            })
        })
        .when_some(
            (!disabled).then_some(on_click).flatten(),
            |element, handler| element.on_click(handler),
        )
}

/// A consistent interactive action component.
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    icon: Option<IconName>,
    aria_label: Option<SharedString>,
    tooltip: Option<SharedString>,
    keyboard_focusable: bool,
    variant: ButtonVariant,
    size: ControlSize,
    state: ControlState,
    full_width: bool,
    theme: UiTheme,
    on_click: Option<ClickHandler>,
}

impl Button {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, theme: UiTheme) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            aria_label: None,
            tooltip: None,
            keyboard_focusable: false,
            variant: ButtonVariant::default(),
            size: ControlSize::default(),
            state: ControlState::default(),
            full_width: false,
            theme,
            on_click: None,
        }
    }

    #[must_use]
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    #[must_use]
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    #[must_use]
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    #[must_use]
    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.aria_label = Some(label.into());
        self
    }

    #[must_use]
    pub fn tooltip(mut self, label: impl Into<SharedString>) -> Self {
        self.tooltip = Some(label.into());
        self
    }

    #[must_use]
    pub fn keyboard_focusable(mut self) -> Self {
        self.keyboard_focusable = true;
        self
    }

    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.state.interaction = if disabled {
            InteractionState::Disabled
        } else {
            InteractionState::Rest
        };
        self
    }

    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.state.selected = selected;
        self
    }

    /// Stretches the button across its container, with its label at the start.
    #[must_use]
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
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

impl Selectable for Button {
    fn selected(self, selected: bool) -> Self {
        Self::selected(self, selected)
    }

    fn is_selected(&self) -> bool {
        self.state.selected
    }
}

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let disabled = self.state.interaction == InteractionState::Disabled;
        let style = self.theme.control_style(self.variant, self.state);
        let hover = self.theme.control_style(
            self.variant,
            ControlState {
                interaction: InteractionState::Hovered,
                selected: self.state.selected,
            },
        );
        let (padding, height, typography) = match self.size {
            ControlSize::Small => (
                tokens::CONTROL_PADDING_X_SMALL,
                tokens::ICON_BUTTON_SIZE,
                Typography::Small,
            ),
            ControlSize::Medium => (
                tokens::CONTROL_PADDING_X_MEDIUM,
                tokens::ROW_HEIGHT,
                Typography::Body,
            ),
        };
        let selector = self.id.to_string();
        let icon_selector = format!("{selector}-icon");
        let theme = self.theme;
        interactive(
            div()
                .id(self.id)
                .debug_selector(move || selector)
                .role(Role::Button)
                .flex_none()
                .h(rem(height))
                .px(rem(padding))
                .flex()
                .items_center()
                .gap_1()
                .map(|element| {
                    if self.full_width {
                        element.w_full()
                    } else {
                        element.justify_center()
                    }
                })
                .rounded(rem(tokens::RADIUS_CONTROL))
                .typography(typography)
                .when(self.keyboard_focusable && !disabled, |element| {
                    element
                        .focusable()
                        .tab_stop(true)
                        .focus_visible(move |style| {
                            style
                                .bg(hover.background.unwrap_or(theme.colors.surface_selected))
                                .text_color(hover.foreground)
                        })
                })
                .when_some(self.tooltip, |element, label| {
                    element.tooltip(Tooltip::text(label, theme))
                })
                .when_some(
                    self.aria_label,
                    gpui::StatefulInteractiveElement::aria_label,
                ),
            style,
            hover,
            disabled,
            self.on_click,
        )
        .children(self.icon.map(|icon| {
            div()
                .flex()
                .debug_selector(move || icon_selector)
                .child(Icon::new(icon, style.foreground).size(tokens::ICON_SIZE_SMALL))
        }))
        .child(div().min_w_0().truncate().child(self.label))
    }
}

#[derive(IntoElement)]
pub struct IconButton {
    id: ElementId,
    icon: IconName,
    label: SharedString,
    selected: bool,
    keyboard_focusable: bool,
    theme: UiTheme,
    on_click: Option<ClickHandler>,
}

impl IconButton {
    #[must_use]
    pub fn new(
        id: impl Into<ElementId>,
        icon: IconName,
        label: impl Into<SharedString>,
        theme: UiTheme,
    ) -> Self {
        Self {
            id: id.into(),
            icon,
            label: label.into(),
            selected: false,
            keyboard_focusable: false,
            theme,
            on_click: None,
        }
    }

    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    #[must_use]
    pub fn keyboard_focusable(mut self) -> Self {
        self.keyboard_focusable = true;
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

impl Selectable for IconButton {
    fn selected(self, selected: bool) -> Self {
        Self::selected(self, selected)
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for IconButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let colors = self.theme.colors;
        let selector = self.id.to_string();
        let group: SharedString = format!("{selector}-hover").into();
        let label = self.label.clone();
        let theme = self.theme;
        let color = if self.selected {
            colors.text
        } else {
            colors.text_muted
        };
        div()
            .id(self.id)
            .group(group.clone())
            .debug_selector(move || selector)
            .role(Role::Button)
            .aria_label(self.label)
            .flex_none()
            .size(rem(tokens::ICON_BUTTON_SIZE))
            .flex()
            .items_center()
            .justify_center()
            .rounded(rem(tokens::RADIUS_CONTROL))
            .cursor_pointer()
            .when(self.selected, |button| button.bg(colors.surface_selected))
            .hover(move |button| button.bg(colors.surface_hover))
            .when(self.keyboard_focusable, |button| {
                button
                    .focusable()
                    .tab_stop(true)
                    .focus_visible(move |style| style.bg(colors.surface_selected))
            })
            .tooltip(Tooltip::text(label, theme))
            .when_some(self.on_click, |button, handler| {
                button.on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    handler(event, window, cx);
                })
            })
            .child(Icon::new(self.icon, color).group_hover(group, colors.text))
    }
}

pub struct Tooltip {
    label: SharedString,
    theme: UiTheme,
}

impl Tooltip {
    pub fn text(
        label: impl Into<SharedString>,
        theme: UiTheme,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let label = label.into();
        move |_, cx| {
            cx.new(|_| Tooltip {
                label: label.clone(),
                theme,
            })
            .into()
        }
    }
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let colors = self.theme.colors;
        div()
            .px_2()
            .py_1()
            .rounded(rem(tokens::RADIUS_CONTAINER))
            .border_1()
            .border_color(colors.border)
            .bg(colors.surface)
            .shadow_md()
            .typography(Typography::Small)
            .text_color(colors.text)
            .child(self.label.clone())
    }
}

#[derive(IntoElement)]
pub struct Disclosure {
    id: ElementId,
    expanded: bool,
    theme: UiTheme,
    on_toggle: Option<ClickHandler>,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl Disclosure {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, expanded: bool, theme: UiTheme) -> Self {
        Self {
            id: id.into(),
            expanded,
            theme,
            on_toggle: None,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    #[must_use]
    pub fn on_toggle(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_toggle = Some(Box::new(handler));
        self
    }
}

impl ParentElement for Disclosure {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Disclosure {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Disclosure {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let colors = self.theme.colors;
        let selector = self.id.to_string();
        let chevron = if self.expanded {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        };
        let mut row = div()
            .id(self.id)
            .group(ListRow::hover_group(&selector))
            .debug_selector(move || selector)
            .role(Role::Button)
            .aria_expanded(self.expanded)
            .focusable()
            .tab_stop(true)
            .min_w_0()
            .flex()
            .items_center()
            .gap_2()
            .px_1()
            .rounded(rem(tokens::RADIUS_CONTROL))
            .cursor_pointer()
            .hover(move |row| row.bg(colors.surface_hover))
            .focus_visible(move |row| row.bg(colors.surface_hover))
            .when_some(self.on_toggle, |row, handler| row.on_click(handler))
            .child(Icon::new(chevron, colors.text_muted).size(tokens::ICON_SIZE_SMALL))
            .children(self.children);
        row.style().refine(&self.style);
        row
    }
}

/// Paints any element the way [`Surface`] paints itself, for panels that need their own layout.
pub trait SurfaceExt: Styled + Sized {
    #[must_use]
    fn surface(self, theme: UiTheme) -> Self {
        self.rounded(rem(tokens::RADIUS_CONTAINER))
            .border_1()
            .border_color(theme.colors.border)
            .bg(theme.colors.surface)
    }
}

impl<T: Styled> SurfaceExt for T {}

/// A bordered or elevated content surface.
#[derive(IntoElement)]
pub struct Surface {
    theme: UiTheme,
    padded: bool,
    raised: bool,
    selected: bool,
    children: Vec<AnyElement>,
}

impl Surface {
    #[must_use]
    pub fn new(theme: UiTheme) -> Self {
        Self {
            theme,
            padded: true,
            raised: false,
            selected: false,
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn padded(mut self, padded: bool) -> Self {
        self.padded = padded;
        self
    }
    #[must_use]
    pub fn raised(mut self, raised: bool) -> Self {
        self.raised = raised;
        self
    }
    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl ParentElement for Surface {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Surface {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .w_full()
            .surface(self.theme)
            .when(self.selected, |surface| {
                surface.bg(self.theme.colors.surface_selected)
            })
            .when(self.padded, gpui::Styled::p_3)
            .when(self.raised, gpui::Styled::shadow_lg)
            .children(self.children)
    }
}

/// A selectable row shell with consistent hover and selection treatment.
///
/// Its id doubles as its debug selector, and names the hover group
/// [`ListRow::hover_group`], so children can show themselves only while the
/// pointer is over the row.
#[derive(IntoElement)]
pub struct ListRow {
    id: ElementId,
    theme: UiTheme,
    state: SelectionState,
    depth: usize,
    children: Vec<AnyElement>,
    on_click: Option<ClickHandler>,
}

impl ListRow {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, theme: UiTheme) -> Self {
        Self {
            id: id.into(),
            theme,
            state: SelectionState::None,
            depth: 0,
            children: Vec::new(),
            on_click: None,
        }
    }
    /// The hover group of the row with `id`.
    #[must_use]
    pub fn hover_group(id: &str) -> SharedString {
        format!("{id}-hover").into()
    }

    /// Indents the row's content one [`tokens::TREE_INDENT`] per level of
    /// `depth`, while its hover and selection background stays full width.
    #[must_use]
    pub const fn depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }
    #[must_use]
    pub const fn state(mut self, state: SelectionState) -> Self {
        self.state = state;
        self
    }
    #[must_use]
    pub const fn selected(self, selected: bool) -> Self {
        self.state(if selected {
            SelectionState::Selected
        } else {
            SelectionState::None
        })
    }
    #[must_use]
    pub const fn disabled(self, disabled: bool) -> Self {
        self.state(if disabled {
            SelectionState::Disabled
        } else {
            SelectionState::None
        })
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

impl Selectable for ListRow {
    fn selected(self, selected: bool) -> Self {
        Self::selected(self, selected)
    }

    fn is_selected(&self) -> bool {
        self.state == SelectionState::Selected
    }
}

impl ParentElement for ListRow {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for ListRow {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let style = self.theme.selection_style(self.state);
        let hover = if self.state == SelectionState::None {
            self.theme.control_style(
                ButtonVariant::Ghost,
                ControlState::new(InteractionState::Hovered),
            )
        } else {
            style
        };
        let selector = self.id.to_string();
        interactive(
            div()
                .id(self.id)
                .group(Self::hover_group(&selector))
                .debug_selector(move || selector)
                .min_h(rem(tokens::ROW_HEIGHT))
                .w_full()
                .pl(indent(self.depth))
                .pr_2()
                .flex()
                .items_center()
                .gap_2()
                .rounded(rem(tokens::RADIUS_CONTROL))
                .typography(Typography::Body),
            style,
            hover,
            self.state == SelectionState::Disabled,
            self.on_click,
        )
        .children(self.children)
    }
}

/// Standard bottom action bar.
#[derive(IntoElement)]
pub struct ActionBar {
    theme: UiTheme,
    children: Vec<AnyElement>,
}
impl ActionBar {
    #[must_use]
    pub fn new(theme: UiTheme) -> Self {
        Self {
            theme,
            children: Vec::new(),
        }
    }
}
impl ParentElement for ActionBar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}
impl RenderOnce for ActionBar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .h(rem(tokens::BAR_HEIGHT))
            .w_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .border_t_1()
            .border_color(self.theme.colors.border)
            .typography(Typography::Small)
            .children(self.children)
    }
}

#[derive(IntoElement)]
pub struct Backdrop {
    id: SharedString,
    theme: UiTheme,
    align_top: bool,
    on_dismiss: Option<DismissHandler>,
    children: Vec<AnyElement>,
}
impl Backdrop {
    #[must_use]
    pub fn new(id: impl Into<SharedString>, theme: UiTheme) -> Self {
        Self {
            id: id.into(),
            theme,
            align_top: false,
            on_dismiss: None,
            children: Vec::new(),
        }
    }
    #[must_use]
    pub fn align_top(mut self) -> Self {
        self.align_top = true;
        self
    }
    #[must_use]
    pub fn on_dismiss(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Box::new(handler));
        self
    }
}
impl ParentElement for Backdrop {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}
impl RenderOnce for Backdrop {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let margin = rem(tokens::MODAL_VIEWPORT_MARGIN / 2.0);
        let selector = self.id.to_string();
        let on_dismiss = self.on_dismiss;
        div()
            .id(self.id)
            .debug_selector(move || selector)
            .absolute()
            .inset_0()
            .p(margin)
            .when(self.align_top, |backdrop| {
                backdrop.pt(rem(tokens::MODAL_TOP_OFFSET))
            })
            .flex()
            .flex_col()
            .items_center()
            .when(!self.align_top, gpui::Styled::justify_center)
            .bg(self.theme.colors.scrim)
            .occlude()
            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                if let Some(dismiss) = &on_dismiss {
                    dismiss(window, cx);
                }
            })
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .children(self.children)
    }
}

/// Centered modal overlay and panel.
#[derive(IntoElement)]
pub struct Modal {
    id: SharedString,
    title: SharedString,
    hint: Option<SharedString>,
    theme: UiTheme,
    width: Rems,
    height: Option<Rems>,
    align_top: bool,
    on_dismiss: Option<DismissHandler>,
    children: Vec<AnyElement>,
}
impl Modal {
    #[must_use]
    pub fn new(
        id: impl Into<SharedString>,
        title: impl Into<SharedString>,
        theme: UiTheme,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            hint: None,
            theme,
            width: rem(tokens::MODAL_WIDTH),
            height: None,
            align_top: false,
            on_dismiss: None,
            children: Vec::new(),
        }
    }
    #[must_use]
    pub fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }
    #[must_use]
    pub fn width(mut self, width: Rems) -> Self {
        self.width = width;
        self
    }
    #[must_use]
    pub fn height(mut self, height: Rems) -> Self {
        self.height = Some(height);
        self
    }
    #[must_use]
    pub fn size(mut self, size: ModalSize) -> Self {
        self.width = rem(match size {
            ModalSize::Compact => tokens::MODAL_WIDTH_COMPACT,
            ModalSize::Medium => tokens::MODAL_WIDTH,
            ModalSize::Wide => tokens::MODAL_WIDTH_WIDE,
        });
        self
    }
    #[must_use]
    pub fn align_top(mut self) -> Self {
        self.align_top = true;
        self
    }
    #[must_use]
    pub fn on_dismiss(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Box::new(handler));
        self
    }
}
impl ParentElement for Modal {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}
impl RenderOnce for Modal {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let colors = self.theme.colors;
        let selector = self.id.to_string();
        let mut backdrop = Backdrop::new(format!("{}-backdrop", self.id), self.theme);
        backdrop.align_top = self.align_top;
        backdrop.on_dismiss = self.on_dismiss;
        backdrop.child(
            div()
                .id(self.id)
                .debug_selector(move || selector)
                .occlude()
                .w(self.width)
                .max_w_full()
                .when_some(self.height, gpui::Styled::h)
                .max_h_full()
                .min_h_0()
                .p_4()
                .rounded(rem(tokens::RADIUS_CONTAINER))
                .flex()
                .flex_col()
                .border_1()
                .border_color(colors.border)
                .bg(colors.surface)
                .shadow_lg()
                .typography(Typography::Body)
                .child(
                    div()
                        .mb_3()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().typography(Typography::Title).child(self.title))
                        .children(self.hint.map(|hint| {
                            div()
                                .typography(Typography::Small)
                                .text_color(colors.text_muted)
                                .child(hint)
                        })),
                )
                .children(self.children),
        )
    }
}

/// Secondary text using the standard muted color.
#[derive(IntoElement)]
pub struct MutedText {
    text: SharedString,
    theme: UiTheme,
}

impl MutedText {
    #[must_use]
    pub fn new(text: impl Into<SharedString>, theme: UiTheme) -> Self {
        Self {
            text: text.into(),
            theme,
        }
    }
}

impl RenderOnce for MutedText {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .text_color(self.theme.colors.text_muted)
            .child(self.text)
    }
}

/// Full-area empty, loading, or failure state, drawn over its container's
/// background.
#[derive(IntoElement)]
pub struct EmptyState {
    title: SharedString,
    detail: SharedString,
    tone: NoticeTone,
    theme: UiTheme,
}
impl EmptyState {
    #[must_use]
    pub fn new(
        title: impl Into<SharedString>,
        detail: impl Into<SharedString>,
        tone: NoticeTone,
        theme: UiTheme,
    ) -> Self {
        Self {
            title: title.into(),
            detail: detail.into(),
            tone,
            theme,
        }
    }
}
impl RenderOnce for EmptyState {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let style = self.theme.notice_style(self.tone);
        div()
            .size_full()
            .p_6()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_1()
            .text_center()
            .text_color(style.foreground)
            .child(div().typography(Typography::Heading).child(self.title))
            .child(
                div()
                    .typography(Typography::Body)
                    .text_color(self.theme.colors.text_muted)
                    .child(self.detail),
            )
    }
}

/// Semantic notification banner.
#[derive(IntoElement)]
pub struct Notification {
    message: SharedString,
    tone: NoticeTone,
    theme: UiTheme,
}
impl Notification {
    #[must_use]
    pub fn new(message: impl Into<SharedString>, tone: NoticeTone, theme: UiTheme) -> Self {
        Self {
            message: message.into(),
            tone,
            theme,
        }
    }

    #[must_use]
    pub fn error(message: impl Into<SharedString>, theme: UiTheme) -> Self {
        Self::new(message, NoticeTone::Error, theme)
    }
}
impl RenderOnce for Notification {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let style = self.theme.notice_style(self.tone);
        div()
            .w_full()
            .p_3()
            .rounded(rem(tokens::RADIUS_CONTAINER))
            .border_1()
            .border_color(style.foreground)
            .bg(self.theme.colors.surface)
            .text_color(style.foreground)
            .typography(Typography::Body)
            .when(style.emphasized, |notification| {
                notification.font_weight(gpui::FontWeight::SEMIBOLD)
            })
            .child(self.message)
    }
}
