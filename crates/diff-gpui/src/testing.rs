//! Reusable GPUI integration-test support.
//!
//! The harness opens the production [`DiffViewer`] in a real
//! GPUI test window, records host events, drives input through GPUI, and exposes
//! rendered element bounds. It deliberately does not use image snapshots.

use crate::{DiffViewer, DiffViewerEvent, DiffViewerOptions};
use clankerdiff_core::{DiffDocument, DiffReviewCommand, testing::DocumentBuilder};
use clankerdiff_theme::ReviewTheme;
use gpui::{
    AnyWindowHandle, App, Bounds, Context, Entity, InputEvent, ListOffset, Pixels, Point, Render,
    ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase, VisualTestContext, Window,
    WindowHandle, WindowOptions, div, prelude::*,
};
use std::{error::Error, sync::Arc};

struct HarnessRoot {
    viewer: Entity<DiffViewer>,
    events: Vec<DiffViewerEvent>,
}

impl HarnessRoot {
    fn new(
        document: Arc<DiffDocument>,
        theme: ReviewTheme,
        options: DiffViewerOptions,
        cx: &mut Context<Self>,
    ) -> Self {
        let viewer = cx.new(|_| DiffViewer::with_options(document, theme, options));
        cx.subscribe(&viewer, |root, _, event: &DiffViewerEvent, _| {
            root.events.push(event.clone());
        })
        .detach();
        Self {
            viewer,
            events: Vec::new(),
        }
    }
}

impl Render for HarnessRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.viewer.clone())
    }
}

/// Struct-update-friendly configuration for a [`DiffViewerHarness`].
///
/// ```
/// use clankerdiff_core::testing::DocumentBuilder;
/// use clankerdiff_gpui::testing::DiffViewerHarnessBuilder;
///
/// let builder = DiffViewerHarnessBuilder {
///     document: DocumentBuilder::new()
///         .changed("src/lib.rs", "old\n", "new\n")
///         .build(),
///     ..DiffViewerHarnessBuilder::default()
/// };
/// # let _ = builder;
/// ```
pub struct DiffViewerHarnessBuilder {
    pub document: Arc<DiffDocument>,
    pub theme: ReviewTheme,
    pub options: DiffViewerOptions,
    pub window_options: WindowOptions,
}

impl Default for DiffViewerHarnessBuilder {
    fn default() -> Self {
        Self {
            document: DocumentBuilder::new()
                .changed("src/lib.rs", "old\n", "new\n")
                .build(),
            theme: ReviewTheme::default(),
            options: DiffViewerOptions::default(),
            window_options: WindowOptions::default(),
        }
    }
}

impl DiffViewerHarnessBuilder {
    /// Opens the configured viewer and settles the initial frame.
    ///
    /// # Panics
    ///
    /// Panics if GPUI cannot open, read, or draw the test window.
    pub fn build(self, cx: &mut TestAppContext) -> DiffViewerHarness {
        cx.update(DiffViewer::bind_keys);
        let Self {
            document,
            theme,
            options,
            window_options,
        } = self;
        let window = cx.update(|cx| {
            cx.open_window(window_options, |_, cx| {
                cx.new(|cx| HarnessRoot::new(document, theme, options, cx))
            })
            .expect("open GPUI test window")
        });
        cx.run_until_parked();
        let viewer = window
            .read_with(cx, |root, _| root.viewer.clone())
            .expect("read GPUI test root");
        let harness = DiffViewerHarness { window, viewer };
        harness.draw(cx);
        harness
    }
}

/// A high-level integration harness around a rendered GPUI diff viewer.
pub struct DiffViewerHarness {
    window: WindowHandle<HarnessRoot>,
    viewer: Entity<DiffViewer>,
}

impl DiffViewerHarness {
    #[must_use]
    pub fn viewer(&self) -> Entity<DiffViewer> {
        self.viewer.clone()
    }

    #[must_use]
    pub fn window(&self) -> AnyWindowHandle {
        *self.window
    }

    /// Draws and settles the GPUI test window.
    ///
    /// # Panics
    ///
    /// Panics if GPUI cannot update or draw the test window.
    pub fn draw(&self, cx: &mut TestAppContext) {
        cx.update_window(*self.window, |_, window, cx| window.draw(cx).clear(cx))
            .expect("draw GPUI test window");
        cx.run_until_parked();
    }

    pub fn dispatch_command(&self, cx: &mut TestAppContext, command: impl Into<DiffReviewCommand>) -> Result<bool, Box<dyn Error>> {
        let command = command.into();
        let handled = cx.update_window(*self.window, |_, window, cx| {
            self.viewer.update(cx, |viewer, cx| viewer.handle_command(command, window, cx))
        })?;
        self.draw(cx);
        Ok(handled)
    }

    pub fn simulate_keystrokes(&self, cx: &mut TestAppContext, keystrokes: &str) {
        cx.simulate_keystrokes(*self.window, keystrokes);
        self.draw(cx);
    }

    /// Dispatches a synthetic scroll-wheel event, then settles the frame.
    ///
    /// # Panics
    ///
    /// Panics if GPUI cannot update or draw the test window.
    pub fn simulate_scroll(
        &self,
        cx: &mut TestAppContext,
        position: Point<Pixels>,
        delta: Point<Pixels>,
    ) {
        cx.update_window(*self.window, |_, window, cx| {
            window.dispatch_event(
                ScrollWheelEvent {
                    position,
                    delta: ScrollDelta::Pixels(delta),
                    touch_phase: TouchPhase::Moved,
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
        })
        .expect("dispatch test scroll wheel event");
        self.draw(cx);
    }

    /// Returns the center of the element painted under a debug selector.
    ///
    /// # Panics
    ///
    /// Panics if the selector was never painted.
    #[must_use]
    pub fn scroll_center(&self, cx: &mut TestAppContext, selector: &'static str) -> Point<Pixels> {
        let bounds = self
            .bounds(cx, selector)
            .unwrap_or_else(|| panic!("missing painted bounds for {selector}"));
        Point::new(
            bounds.origin.x + bounds.size.width / 2.0,
            bounds.origin.y + bounds.size.height / 2.0,
        )
    }

    #[must_use]
    pub fn diff_scroll_top(&self, cx: &TestAppContext) -> ListOffset {
        self.read(cx, |viewer, _| viewer.diff_scroll_top())
    }

    pub fn scroll_to_bottom_of_diff(&self, cx: &mut TestAppContext) {
        self.update(cx, DiffViewer::scroll_diff_to_end);
        self.draw(cx);
    }

    /// Returns the host events recorded by the rendered viewer.
    ///
    /// # Panics
    ///
    /// Panics if GPUI cannot read the test window.
    #[must_use]
    pub fn events(&self, cx: &TestAppContext) -> Vec<DiffViewerEvent> {
        self.window
            .read_with(cx, |root, _| root.events.clone())
            .expect("read recorded GPUI events")
    }

    /// Returns the rendered bounds registered by a stable debug selector.
    pub fn bounds(
        &self,
        cx: &mut TestAppContext,
        selector: &'static str,
    ) -> Option<Bounds<gpui::Pixels>> {
        self.draw(cx);
        let mut visual = VisualTestContext::from_window(*self.window, cx);
        visual.debug_bounds(selector)
    }

    /// Reads the viewer while keeping GPUI context plumbing inside the harness.
    pub fn read<T>(&self, cx: &TestAppContext, read: impl FnOnce(&DiffViewer, &App) -> T) -> T {
        self.viewer.read_with(cx, read)
    }

    /// Updates the viewer while keeping GPUI context plumbing inside the harness.
    pub fn update<T>(
        &self,
        cx: &mut TestAppContext,
        update: impl FnOnce(&mut DiffViewer, &mut Context<DiffViewer>) -> T,
    ) -> T {
        self.viewer.update(cx, update)
    }
}
