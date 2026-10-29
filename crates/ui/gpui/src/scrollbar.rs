use crate::{
    theme::UiTheme,
    tokens::{SCROLLBAR_MIN_THUMB_HEIGHT, SCROLLBAR_WIDTH, rem},
};
use gpui::{
    App, DragMoveEvent, ElementId, Empty, IntoElement, ListState, MouseButton, MouseDownEvent, Pixels, RenderOnce,
    Window, div, point, prelude::*, px,
};
use std::cell::Cell;

/// A vertical scrollbar for a [`gpui::list`], placed beside it as a sibling of the same height.
///
/// Clicking the track jumps there and dragging the thumb scrolls. It renders nothing while the
/// list fits its viewport. The track and thumb are tagged `{id}` and `{id}-thumb`.
#[derive(IntoElement)]
pub struct ListScrollbar {
    id: ElementId,
    list: ListState,
    theme: UiTheme,
}

impl ListScrollbar {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, list: ListState, theme: UiTheme) -> Self {
        Self { id: id.into(), list, theme }
    }
}

impl RenderOnce for ListScrollbar {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let list = self.list;
        let viewport_height = list.viewport_bounds().size.height;
        if viewport_height <= px(0.0) {
            // The list lays out after its first paint; draw again once its viewport is known.
            if list.item_count() != 0 {
                window.request_animation_frame();
            }
            return Empty.into_any_element();
        }
        if list.max_offset_for_scrollbar().y <= px(0.0) {
            return Empty.into_any_element();
        }

        let min_thumb_height = rem(SCROLLBAR_MIN_THUMB_HEIGHT).to_pixels(window.rem_size());
        let thumb_height = thumb_height(viewport_height, min_thumb_height);
        let thumb_top = (viewport_height - thumb_height) * scroll_fraction(&list);
        let colors = self.theme.colors;
        let selector = self.id.to_string();
        let thumb_selector = format!("{selector}-thumb");
        let drag = ScrollbarDrag {
            scrollbar: self.id.clone(),
            thumb_offset: Cell::new(px(0.0)),
            min_thumb_height,
            list: list.clone(),
            fraction: Cell::new(None),
        };
        let scrollbar = self.id.clone();
        let click_list = list.clone();

        div()
            .id(self.id)
            .debug_selector(move || selector)
            .w(rem(SCROLLBAR_WIDTH))
            .h_full()
            .flex_shrink_0()
            .relative()
            .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, window, cx| {
                set_offset_from_pointer(&click_list, event.position.y, thumb_height / 2.0, min_thumb_height);
                cx.stop_propagation();
                window.refresh();
            })
            .on_drag_move(move |event: &DragMoveEvent<ScrollbarDrag>, window, cx| {
                let drag = event.drag(cx);
                if drag.scrollbar != scrollbar {
                    return;
                }
                if let Some(fraction) = set_offset_from_pointer(
                    &drag.list,
                    event.event.position.y,
                    drag.thumb_offset.get(),
                    drag.min_thumb_height,
                ) {
                    drag.fraction.set(Some(fraction));
                }
                window.refresh();
            })
            .child(
                div()
                    .id(ElementId::Name(thumb_selector.clone().into()))
                    .debug_selector(move || thumb_selector)
                    .absolute()
                    .top(thumb_top)
                    .right(rem(SCROLLBAR_INSET))
                    .w(rem(SCROLLBAR_THUMB_WIDTH))
                    .h(thumb_height)
                    .rounded_full()
                    .cursor_pointer()
                    .bg(colors.text_muted.opacity(THUMB_REST_OPACITY))
                    .hover(move |thumb| thumb.bg(colors.text_muted))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_drag(drag, |drag, click_offset, _, cx| {
                        drag.start(click_offset.y);
                        cx.new(|_| Empty)
                    }),
            )
            .into_any_element()
    }
}

const SCROLLBAR_THUMB_WIDTH: f32 = 6.0;
const SCROLLBAR_INSET: f32 = 3.0;
const THUMB_REST_OPACITY: f32 = 0.4;

struct ScrollbarDrag {
    scrollbar: ElementId,
    thumb_offset: Cell<Pixels>,
    min_thumb_height: Pixels,
    list: ListState,
    fraction: Cell<Option<f32>>,
}

impl ScrollbarDrag {
    fn start(&self, thumb_offset: Pixels) {
        self.thumb_offset.set(thumb_offset);
        self.list.scrollbar_drag_started();
        self.fraction.set(Some(scroll_fraction(&self.list)));
    }
}

impl Drop for ScrollbarDrag {
    fn drop(&mut self) {
        if let Some(fraction) = self.fraction.get() {
            self.list.scrollbar_drag_ended();
            set_scroll_fraction(&self.list, fraction);
        }
    }
}

fn thumb_height(viewport_height: Pixels, min_thumb_height: Pixels) -> Pixels {
    // Variable-list content height is estimated until every row has been measured. A fixed thumb
    // avoids visibly shrinking as wrapped rows are discovered while preserving useful drag space.
    min_thumb_height.min(viewport_height / 2.0)
}

fn scroll_fraction(list: &ListState) -> f32 {
    let max_offset = list.max_offset_for_scrollbar().y;
    if max_offset <= px(0.0) {
        return 0.0;
    }
    (-list.scroll_px_offset_for_scrollbar().y / max_offset).clamp(0.0, 1.0)
}

fn set_scroll_fraction(list: &ListState, fraction: f32) {
    list.set_offset_from_scrollbar(point(px(0.0), -list.max_offset_for_scrollbar().y * fraction));
}

fn set_offset_from_pointer(
    list: &ListState,
    pointer_y: Pixels,
    thumb_offset: Pixels,
    min_thumb_height: Pixels,
) -> Option<f32> {
    let viewport = list.viewport_bounds();
    let max_offset = list.max_offset_for_scrollbar().y;
    let thumb_height = thumb_height(viewport.size.height, min_thumb_height);
    let track_space = viewport.size.height - thumb_height;
    if max_offset <= px(0.0) || track_space <= px(0.0) {
        return None;
    }

    let fraction = (pointer_y - viewport.origin.y - thumb_offset).clamp(px(0.0), track_space) / track_space;
    set_scroll_fraction(list, fraction);
    Some(fraction)
}
