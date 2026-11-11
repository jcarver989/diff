use crate::tokens::{SCROLLBAR_MIN_THUMB_HEIGHT, SCROLLBAR_WIDTH, scaled};
use rouge_core::{BuildCx, ElementId, ScrollState, View, VirtualListHandle};
use rouge_reactive::Signal;
use rouge_widgets::{ScrollbarStyle, vertical_scrollbar};

pub struct ListScrollbar {
    state: Signal<ScrollState>,
}
impl ListScrollbar {
    #[must_use]
    pub fn new(state: Signal<ScrollState>) -> Self {
        Self { state }
    }

    #[must_use]
    pub fn list(list: VirtualListHandle) -> Self {
        Self::new(list.scroll_state())
    }
}
impl View for ListScrollbar {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let root = rouge_core::root_font_size();
        vertical_scrollbar(self.state, move || {
            let scale = root.get() / crate::tokens::BASE_SIZE;
            ScrollbarStyle {
                track: SCROLLBAR_WIDTH * scale,
                inset: 2.0 * scale,
                thumb: 8.0 * scale,
                min_thumb: SCROLLBAR_MIN_THUMB_HEIGHT * scale,
                radius: 4.0 * scale,
            }
        })
        .build(cx)
    }
}

pub fn uniform_height_hints(design_px: f32) -> impl Fn() -> f32 {
    scaled(design_px)
}
