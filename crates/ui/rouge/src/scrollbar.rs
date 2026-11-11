use crate::tokens::{SCROLLBAR_MIN_THUMB_HEIGHT, SCROLLBAR_WIDTH};
use rouge_core::{BuildCx, ElementId, Length, ScrollState, View, VirtualListHandle};
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
        let style = ScrollbarStyle {
            track: Length::design(SCROLLBAR_WIDTH),
            inset: Length::design(2.0),
            thumb: Length::design(8.0),
            min_thumb: Length::design(SCROLLBAR_MIN_THUMB_HEIGHT),
            radius: Length::design(4.0),
        };
        vertical_scrollbar(self.state, style).build(cx)
    }
}
