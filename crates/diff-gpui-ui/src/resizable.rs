use crate::{
    theme::UiTheme,
    tokens::{RESIZE_HANDLE_WIDTH, rem},
};
use gpui::{
    AnyElement, App, ClickEvent, DragMoveEvent, ElementId, Empty, IntoElement, ParentElement,
    Refineable, Rems, RenderOnce, StyleRefinement, Styled, Window, div, prelude::*, rems,
};

type ResizeHandler = Box<dyn Fn(Rems, &mut Window, &mut App) + 'static>;
type ResetHandler = Box<dyn Fn(&mut Window, &mut App) + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelSide {
    Left,
    Right,
}

#[derive(Debug, Clone)]
struct ResizeDrag {
    panel: ElementId,
}

#[derive(IntoElement)]
pub struct ResizablePanel {
    id: ElementId,
    side: PanelSide,
    width: Rems,
    min_width: Rems,
    max_width: Rems,
    theme: UiTheme,
    style: StyleRefinement,
    on_resize: Option<ResizeHandler>,
    on_reset: Option<ResetHandler>,
    children: Vec<AnyElement>,
}

impl ResizablePanel {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, side: PanelSide, width: Rems, theme: UiTheme) -> Self {
        Self {
            id: id.into(),
            side,
            width,
            min_width: Rems::ZERO,
            max_width: rems(f32::INFINITY),
            theme,
            style: StyleRefinement::default(),
            on_resize: None,
            on_reset: None,
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn min_width(mut self, width: Rems) -> Self {
        self.min_width = width;
        self
    }

    #[must_use]
    pub fn max_width(mut self, width: Rems) -> Self {
        self.max_width = width;
        self
    }

    #[must_use]
    pub fn on_resize(mut self, handler: impl Fn(Rems, &mut Window, &mut App) + 'static) -> Self {
        self.on_resize = Some(Box::new(handler));
        self
    }

    #[must_use]
    pub fn on_reset(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_reset = Some(Box::new(handler));
        self
    }
}

impl Styled for ResizablePanel {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for ResizablePanel {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for ResizablePanel {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let colors = self.theme.colors;
        let side = self.side;
        let (min_width, max_width) = (self.min_width, rems(self.max_width.0.max(self.min_width.0)));
        let selector = self.id.to_string();
        let handle_selector = format!("{selector}-resize-handle");
        let drag = ResizeDrag {
            panel: self.id.clone(),
        };
        let panel = self.id.clone();
        let on_resize = self.on_resize;
        let on_reset = self.on_reset;

        let mut element = div()
            .id(self.id)
            .debug_selector(move || selector)
            .relative()
            .flex_none()
            .h_full()
            .w(self.width)
            .flex()
            .flex_col()
            .border_color(colors.border)
            .map(|element| match side {
                PanelSide::Left => element.border_r_1(),
                PanelSide::Right => element.border_l_1(),
            });
        element.style().refine(&self.style);

        element
            .when_some(on_resize, |element, on_resize| {
                element.on_drag_move(move |event: &DragMoveEvent<ResizeDrag>, window, cx| {
                    if event.drag(cx).panel != panel {
                        return;
                    }
                    let pointer = event.event.position.x;
                    let width = match side {
                        PanelSide::Left => pointer - event.bounds.left(),
                        PanelSide::Right => event.bounds.right() - pointer,
                    };
                    let width = rems(width / window.rem_size());
                    on_resize(clamp(width, min_width, max_width), window, cx);
                })
            })
            .children(self.children)
            .child(
                div()
                    .id(ElementId::Name(handle_selector.clone().into()))
                    .debug_selector(move || handle_selector)
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .map(|handle| match side {
                        PanelSide::Left => handle.right_0(),
                        PanelSide::Right => handle.left_0(),
                    })
                    .w(rem(RESIZE_HANDLE_WIDTH))
                    .cursor_col_resize()
                    .hover(move |handle| handle.bg(colors.accent.opacity(0.5)))
                    .on_click(move |event: &ClickEvent, window, cx| {
                        if event.click_count() >= 2
                            && let Some(on_reset) = &on_reset
                        {
                            on_reset(window, cx);
                        }
                        cx.stop_propagation();
                    })
                    .on_drag(drag, |_, _, _, cx| cx.new(|_| Empty)),
            )
    }
}

fn clamp(width: Rems, min: Rems, max: Rems) -> Rems {
    rems(width.0.clamp(min.0, max.0))
}
