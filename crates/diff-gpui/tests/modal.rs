//! Scroll behavior contracts for modal overlays.
//!
//! An open modal must capture scrolling so that wheel input over the modal
//! scrolls the modal's own scrollable regions instead of the content behind
//! the backdrop.

use diff_gpui::ui::{components::Modal, theme::UiTheme};
use diff_theme::DiffTheme;
use gpui::{
    Context, Pixels, Point, Render, ScrollHandle, ScrollWheelEvent, TestAppContext, Window, div,
    point, prelude::*, px, size,
};

struct TestRoot {
    background_scroll: ScrollHandle,
    modal_scroll: ScrollHandle,
    modal: bool,
}

impl TestRoot {
    fn new() -> Self {
        Self {
            background_scroll: ScrollHandle::new(),
            modal_scroll: ScrollHandle::new(),
            modal: false,
        }
    }
}

impl Render for TestRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut root = div().id("test-root").size_full().relative().child(
            div()
                .id("background")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&self.background_scroll)
                .child(div().h(px(2_000.0)).w_full()),
        );
        if self.modal {
            root = root.child(
                Modal::new(
                    "test-modal",
                    "Select theme",
                    UiTheme::new(&DiffTheme::default()),
                )
                .child(
                    div()
                        .id("modal-content")
                        .w(px(400.0))
                        .h(px(400.0))
                        .overflow_y_scroll()
                        .track_scroll(&self.modal_scroll)
                        .child(div().h(px(1_000.0)).w_full()),
                ),
            );
        }
        root
    }
}

fn open_root(cx: &mut TestAppContext) -> gpui::WindowHandle<TestRoot> {
    let window = cx.open_window(size(px(800.0), px(600.0)), |_, _| TestRoot::new());
    cx.run_until_parked();
    window
}

fn show_modal(cx: &mut TestAppContext, window: gpui::WindowHandle<TestRoot>) {
    window
        .update(cx, |root, _, cx| {
            root.modal = true;
            cx.notify();
        })
        .unwrap();
    cx.run_until_parked();
}

fn scroll_at(
    cx: &mut TestAppContext,
    window: gpui::WindowHandle<TestRoot>,
    position: Point<Pixels>,
) {
    window
        .update(cx, |_, window, cx| {
            window.dispatch_event(
                gpui::PlatformInput::ScrollWheel(ScrollWheelEvent {
                    position,
                    delta: gpui::ScrollDelta::Pixels(point(px(0.0), px(-200.0))),
                    ..Default::default()
                }),
                cx,
            );
        })
        .unwrap();
    cx.run_until_parked();
}

#[gpui::test]
fn scrolling_without_a_modal_scrolls_the_background(cx: &mut TestAppContext) {
    let window = open_root(cx);
    let max_offset = window
        .read_with(cx, |root, _| root.background_scroll.max_offset())
        .unwrap();
    assert!(max_offset.y > px(0.0), "the background must be scrollable");

    scroll_at(cx, window, point(px(400.0), px(300.0)));

    let offset = window
        .read_with(cx, |root, _| root.background_scroll.offset())
        .unwrap();
    assert!(
        offset.y < px(0.0),
        "the background scrolls when no modal is open"
    );
}

#[gpui::test]
fn scrolling_over_an_open_modal_does_not_scroll_the_content_behind(cx: &mut TestAppContext) {
    let window = open_root(cx);
    show_modal(cx, window);

    // The modal panel is centered in the 800x600 window, so the center of the
    // window sits over the modal.
    scroll_at(cx, window, point(px(400.0), px(300.0)));

    let offset = window
        .read_with(cx, |root, _| root.background_scroll.offset())
        .unwrap();
    assert_eq!(
        offset.y,
        px(0.0),
        "content behind an open modal must not scroll"
    );
}

#[gpui::test]
fn scrolling_over_an_open_modal_scrolls_the_modal_content_not_the_background(
    cx: &mut TestAppContext,
) {
    let window = open_root(cx);
    show_modal(cx, window);

    scroll_at(cx, window, point(px(400.0), px(300.0)));

    window
        .read_with(cx, |root, _| {
            assert!(
                root.modal_scroll.offset().y < px(0.0),
                "the modal's own scrollable content should take the scroll focus"
            );
            assert_eq!(
                root.background_scroll.offset().y,
                px(0.0),
                "content behind an open modal must not scroll"
            );
        })
        .unwrap();
}
