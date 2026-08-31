//! Reusable GPUI integration-test support.
//!
//! The harness opens the production [`DiffViewer`](crate::DiffViewer) in a real
//! GPUI test window, records host events, drives input through GPUI, and exposes
//! rendered element bounds. It deliberately does not use image snapshots.

use crate::{DiffViewer, DiffViewerEvent, DiffViewerOptions};
use diff_core::{DiffSnapshot, testing::DocumentBuilder};
use diff_theme::DiffTheme;
use gpui::{
    AnyWindowHandle, App, Bounds, Context, Entity, Render, TestAppContext, VisualTestContext,
    Window, WindowHandle, WindowOptions, div, prelude::*,
};

struct HarnessRoot {
    viewer: Entity<DiffViewer>,
    events: Vec<DiffViewerEvent>,
}

impl HarnessRoot {
    fn new(
        snapshot: DiffSnapshot,
        theme: DiffTheme,
        options: DiffViewerOptions,
        cx: &mut Context<Self>,
    ) -> Self {
        let viewer = cx.new(|_| DiffViewer::from_snapshot_with_options(snapshot, theme, options));
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
/// use diff_core::testing::DocumentBuilder;
/// use diff_gpui::testing::DiffViewerHarnessBuilder;
///
/// let builder = DiffViewerHarnessBuilder {
///     snapshot: DocumentBuilder::new()
///         .changed("src/lib.rs", "old\n", "new\n")
///         .build_fixture()
///         .snapshot(),
///     ..DiffViewerHarnessBuilder::default()
/// };
/// # let _ = builder;
/// ```
pub struct DiffViewerHarnessBuilder {
    pub snapshot: DiffSnapshot,
    pub theme: DiffTheme,
    pub options: DiffViewerOptions,
    pub window_options: WindowOptions,
}

impl Default for DiffViewerHarnessBuilder {
    fn default() -> Self {
        Self {
            snapshot: DocumentBuilder::new()
                .changed("src/lib.rs", "old\n", "new\n")
                .build_fixture()
                .snapshot(),
            theme: DiffTheme::default(),
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
            snapshot,
            theme,
            options,
            window_options,
        } = self;
        let window = cx.update(|cx| {
            cx.open_window(window_options, |_, cx| {
                cx.new(|cx| HarnessRoot::new(snapshot, theme, options, cx))
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

    pub fn simulate_keystrokes(&self, cx: &mut TestAppContext, keystrokes: &str) {
        cx.simulate_keystrokes(*self.window, keystrokes);
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
