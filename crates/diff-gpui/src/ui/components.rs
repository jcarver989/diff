//! Reusable, stateless GPUI components.

use super::{
    theme::{UiStyle, UiTheme},
    tokens,
};
pub use diff_theme::{
    ButtonVariant, ControlSize, ControlState, InteractionState, ModalSize, NoticeTone,
    SelectionState,
};
use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, IntoElement, ParentElement, RenderOnce, Role,
    SharedString, Stateful, Window, div, prelude::*, px,
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

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
    aria_label: Option<SharedString>,
    variant: ButtonVariant,
    size: ControlSize,
    disabled: bool,
    selected: bool,
    theme: UiTheme,
    on_click: Option<ClickHandler>,
}

impl Button {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, theme: UiTheme) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            aria_label: None,
            variant: ButtonVariant::default(),
            size: ControlSize::default(),
            disabled: false,
            selected: false,
            theme,
            on_click: None,
        }
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
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
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

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let interaction = if self.disabled {
            InteractionState::Disabled
        } else {
            InteractionState::Rest
        };
        let style = self.theme.control_style(
            self.variant,
            ControlState {
                interaction,
                selected: self.selected,
            },
        );
        let hover = self.theme.control_style(
            self.variant,
            ControlState {
                interaction: InteractionState::Hovered,
                selected: self.selected,
            },
        );
        let padding = match self.size {
            ControlSize::Small => tokens::CONTROL_PADDING_X_SMALL,
            ControlSize::Medium => tokens::CONTROL_PADDING_X_MEDIUM,
        };
        interactive(
            div()
                .id(self.id)
                .role(Role::Button)
                .when_some(
                    self.aria_label,
                    gpui::StatefulInteractiveElement::aria_label,
                )
                .px(px(padding))
                .py(px(tokens::CONTROL_PADDING_Y))
                .rounded_sm(),
            style,
            hover,
            self.disabled,
            self.on_click,
        )
        .child(self.label)
    }
}

/// A compact glyph [`Button`] with a required accessible label.
#[must_use]
pub fn icon_button(
    id: impl Into<ElementId>,
    glyph: impl Into<SharedString>,
    aria_label: impl Into<SharedString>,
    theme: UiTheme,
) -> Button {
    Button::new(id, glyph, theme)
        .size(ControlSize::Small)
        .aria_label(aria_label)
}

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
        let colors = self.theme.colors;
        div()
            .w_full()
            .rounded_md()
            .border_1()
            .border_color(colors.border)
            .bg(if self.selected {
                colors.surface_selected
            } else {
                colors.surface
            })
            .when(self.padded, gpui::Styled::p_3)
            .when(self.raised, gpui::Styled::shadow_lg)
            .children(self.children)
    }
}

/// A selectable row shell with consistent hover and selection treatment.
#[derive(IntoElement)]
pub struct ListRow {
    id: ElementId,
    theme: UiTheme,
    state: SelectionState,
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
            children: Vec::new(),
            on_click: None,
        }
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
        interactive(
            div()
                .id(self.id)
                .min_h(px(tokens::ROW_HEIGHT))
                .w_full()
                .px_3()
                .py_1()
                .flex()
                .items_center(),
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
            .h(px(tokens::TOOLBAR_HEIGHT))
            .w_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .border_t_1()
            .border_color(self.theme.colors.border)
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
    width: f32,
    height: Option<f32>,
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
            width: tokens::MODAL_WIDTH,
            height: None,
            children: Vec::new(),
        }
    }
    #[must_use]
    pub fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }
    #[must_use]
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
    #[must_use]
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }
    #[must_use]
    pub fn size(mut self, size: ModalSize) -> Self {
        self.width = match size {
            ModalSize::Compact => tokens::MODAL_WIDTH_COMPACT,
            ModalSize::Medium => tokens::MODAL_WIDTH,
            ModalSize::Wide => tokens::MODAL_WIDTH_WIDE,
        };
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
        div()
            .id(format!("{}-backdrop", self.id))
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(colors.scrim)
            // A modal captures input: occlude the content behind the backdrop so
            // that scroll wheels (and other mouse input) target the modal's own
            // scrollable regions instead of scrolling the content underneath.
            .occlude()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id(self.id)
                    .w(px(self.width))
                    .max_w_full()
                    .when_some(self.height, |panel, height| panel.h(px(height)))
                    .min_h_0()
                    .p_4()
                    .rounded_md()
                    .flex()
                    .flex_col()
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.surface)
                    .shadow_lg()
                    .child(
                        div()
                            .mb_3()
                            .flex_shrink_0()
                            .flex()
                            .justify_between()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(self.title)
                            .children(
                                self.hint
                                    .map(|hint| div().text_color(colors.text_muted).child(hint)),
                            ),
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

/// Full-area empty, loading, or failure state.
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
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .bg(self.theme.colors.canvas)
            .text_color(style.foreground)
            .child(div().text_xl().child(self.title))
            .child(
                div()
                    .text_sm()
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
            .rounded_md()
            .border_1()
            .border_color(style.foreground)
            .bg(self.theme.colors.surface)
            .text_color(style.foreground)
            .when(style.emphasized, |notification| {
                notification.font_weight(gpui::FontWeight::SEMIBOLD)
            })
            .child(self.message)
    }
}
