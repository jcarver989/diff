use rouge_core::{Div, IntoProp, PointerButton, PointerEvent, View, component, window_handle};

pub trait WindowDragExt: Sized {
    #[must_use]
    fn window_drag_area(self) -> Self;
    #[must_use]
    fn no_window_drag(self) -> Self;
}
impl WindowDragExt for Div {
    fn window_drag_area(self) -> Self {
        let window = window_handle();
        self.preserve_focus().on_pointer_down(move |event| {
            if event.button == PointerButton::Primary {
                if event.click_count == 2 {
                    window.zoom();
                } else {
                    window.start_drag();
                }
            }
        })
    }
    fn no_window_drag(self) -> Self {
        self.on_pointer_down(PointerEvent::stop_propagation)
    }
}

#[component]
pub fn decorate<T: View, U>(title: impl IntoProp<String, U>, children: T) -> Div {
    rouge_widgets::window_chrome(title, children)
}
