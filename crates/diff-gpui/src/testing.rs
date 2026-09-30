//! Reusable GPUI integration-test support.
//!
//! The harness opens the production [`DiffViewer`] in a real
//! GPUI test window, records host events, drives input through GPUI, and exposes
//! rendered element bounds. It deliberately does not use image snapshots.

use crate::{
    DiffViewer, DiffViewerEvent, DiffViewerOptions, MarkdownReviewer, MarkdownReviewerOptions,
};
use clankerdiff_core::{
    DiffDocument, DiffReviewCommand, Review, ReviewCommand, ViewMode, testing::DocumentBuilder,
};
use clankerdiff_markdown::{
    MarkdownDocument, MarkdownReview, MarkdownReviewCommand, MarkdownTargetId,
};
use clankerdiff_theme::ReviewTheme;
use gpui::{
    Action, AnyWindowHandle, App, Bounds, Context, ElementInputHandler, Entity, Focusable,
    InputEvent, InputHandler, KeyDownEvent, KeyUpEvent, Keystroke, ListOffset, Modifiers, Pixels,
    Point, Render, ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase, VisualTestContext,
    Window, WindowBounds, WindowHandle, WindowOptions, div, point, prelude::*, px, size,
};
use gpui_base::input::TextareaState;
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
    pub comments: Vec<String>,
    pub view_mode: Option<ViewMode>,
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
            comments: Vec::new(),
            view_mode: None,
        }
    }
}

impl DiffViewerHarnessBuilder {
    #[must_use]
    pub fn with_comments<T: Into<String>>(mut self, comments: impl IntoIterator<Item = T>) -> Self {
        self.comments = comments.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn with_view_mode(mut self, view_mode: ViewMode) -> Self {
        self.view_mode = Some(view_mode);
        self
    }

    #[must_use]
    pub fn with_window_size(mut self, width: f32, height: f32) -> Self {
        self.window_options.window_bounds = Some(WindowBounds::Windowed(Bounds::new(
            point(px(0.0), px(0.0)),
            size(px(width), px(height)),
        )));
        self
    }

    pub fn run(
        self,
        cx: &mut TestAppContext,
        test: impl FnOnce(&DiffViewerHarness, &mut TestAppContext) -> Result<(), Box<dyn Error>>,
    ) {
        let harness = self.build(cx);
        run_test(cx, |cx| test(&harness, cx));
    }

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
            comments,
            view_mode,
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
        if let Some(mode) = view_mode {
            harness.update(cx, |viewer, cx| viewer.set_view_mode(mode, cx));
        }
        for body in comments {
            harness.add_comment(cx, body).expect("seed review comment");
        }
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

    pub fn add_comment(
        &self,
        cx: &mut TestAppContext,
        body: impl Into<String>,
    ) -> Result<u64, Box<dyn Error>> {
        let (anchor, text) = self.read(cx, |viewer, _| {
            Ok::<_, Box<dyn Error>>((
                viewer
                    .session()
                    .selected_anchor()
                    .ok_or("missing selected anchor")?,
                viewer
                    .session()
                    .selected_cell()
                    .ok_or("missing selected cell")?
                    .text
                    .to_string(),
            ))
        })?;
        let id = self.update(cx, |viewer, cx| viewer.add_comment(anchor, text, body, cx));
        self.draw(cx);
        Ok(id)
    }

    #[must_use]
    pub fn review(&self, cx: &TestAppContext) -> Review {
        self.read(cx, |viewer, _| viewer.review().clone())
    }

    #[must_use]
    pub fn comment_bodies(&self, cx: &TestAppContext) -> Vec<String> {
        self.review(cx)
            .comments()
            .iter()
            .map(|comment| comment.body.clone())
            .collect()
    }

    #[must_use]
    pub fn selected_row(&self, cx: &TestAppContext) -> Option<usize> {
        self.read(cx, |viewer, _| viewer.session().selected_row())
    }

    #[must_use]
    pub fn draft_body(&self, cx: &TestAppContext) -> Option<String> {
        self.read(cx, |viewer, _| {
            viewer
                .session()
                .draft()
                .map(|draft| draft.body().to_owned())
        })
    }

    pub fn click(
        &self,
        cx: &mut TestAppContext,
        selector: &'static str,
    ) -> Result<(), Box<dyn Error>> {
        click_element(cx, self.window(), selector)
    }

    pub fn press_key(&self, cx: &mut TestAppContext, key: &str) -> Result<(), Box<dyn Error>> {
        press_key(cx, self.window(), key)
    }

    pub fn focus_next(&self, cx: &mut TestAppContext) -> Result<(), Box<dyn Error>> {
        cx.update_window(self.window(), |_, window, cx| window.focus_next(cx))?;
        self.draw(cx);
        Ok(())
    }

    pub fn dispatch_command(
        &self,
        cx: &mut TestAppContext,
        command: impl Into<DiffReviewCommand>,
    ) -> Result<bool, Box<dyn Error>> {
        let command = command.into();
        let handled = cx.update_window(*self.window, |_, window, cx| {
            self.viewer
                .update(cx, |viewer, cx| viewer.handle_command(command, window, cx))
        })?;
        self.draw(cx);
        Ok(handled)
    }

    pub fn dispatch_action(
        &self,
        cx: &mut TestAppContext,
        action: &dyn Action,
    ) -> Result<(), Box<dyn Error>> {
        cx.update_window(*self.window, |_, window, cx| {
            window.dispatch_action(action.boxed_clone(), cx);
        })?;
        self.draw(cx);
        Ok(())
    }

    pub fn simulate_keystrokes(&self, cx: &mut TestAppContext, keystrokes: &str) {
        cx.simulate_keystrokes(*self.window, keystrokes);
        self.draw(cx);
    }

    pub fn simulate_input(&self, cx: &mut TestAppContext, text: &str) {
        cx.simulate_input(*self.window, text);
        self.draw(cx);
    }

    pub fn simulate_event(
        &self,
        cx: &mut TestAppContext,
        event: impl InputEvent,
    ) -> Result<(), Box<dyn Error>> {
        cx.update_window(*self.window, |_, window, cx| {
            window.dispatch_event(event.to_platform_input(), cx);
        })?;
        self.draw(cx);
        Ok(())
    }

    pub fn with_comment_input<T>(
        &self,
        cx: &mut TestAppContext,
        update: impl FnOnce(&mut dyn InputHandler, &mut Window, &mut App) -> T,
    ) -> Result<T, Box<dyn Error>> {
        self.draw(cx);
        let editor = self
            .read(cx, |viewer, _| viewer.comment_editor.clone())
            .ok_or("no comment editor")?;

        let input = editor.read_with(cx, |editor, _| editor.input());
        let bounds = input
            .read_with(cx, |input, _| input.text_bounds())
            .ok_or("comment text not painted")?;
        let result = cx.update_window(*self.window, |_, window, cx| {
            update(&mut ElementInputHandler::new(bounds, input), window, cx)
        })?;
        self.draw(cx);
        Ok(result)
    }

    pub fn with_comment_state<T>(
        &self,
        cx: &mut TestAppContext,
        update: impl FnOnce(&mut TextareaState, &mut Window, &mut Context<TextareaState>) -> T,
    ) -> Result<T, Box<dyn Error>> {
        let input = self
            .read(cx, |viewer, cx| {
                viewer
                    .comment_editor
                    .as_ref()
                    .map(|editor| editor.read(cx).input())
            })
            .ok_or("no comment editor")?;
        let result = cx.update_window(*self.window, |_, window, cx| {
            input.update(cx, |input, cx| update(input, window, cx))
        })?;
        self.draw(cx);
        Ok(result)
    }

    pub fn comment_focused(&self, cx: &mut TestAppContext) -> Result<bool, Box<dyn Error>> {
        self.with_comment_state(cx, |input, window, cx| {
            input.focus_handle(cx).is_focused(window)
        })
    }

    #[must_use]
    pub fn comment_text_bounds(&self, cx: &TestAppContext) -> Option<Bounds<Pixels>> {
        self.read(cx, |viewer, cx| {
            viewer
                .comment_editor
                .as_ref()
                .and_then(|editor| editor.read(cx).input().read(cx).text_bounds())
        })
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

pub struct MarkdownReviewerHarnessBuilder {
    pub document: Arc<MarkdownDocument>,
    pub theme: ReviewTheme,
    pub options: MarkdownReviewerOptions,
    pub comments: Vec<String>,
}

impl Default for MarkdownReviewerHarnessBuilder {
    fn default() -> Self {
        Self {
            document: Arc::new(MarkdownDocument::parse("# Heading\n\nText")),
            theme: ReviewTheme::default(),
            options: MarkdownReviewerOptions::default(),
            comments: Vec::new(),
        }
    }
}

impl MarkdownReviewerHarnessBuilder {
    #[must_use]
    pub fn with_comments<T: Into<String>>(mut self, comments: impl IntoIterator<Item = T>) -> Self {
        self.comments = comments.into_iter().map(Into::into).collect();
        self
    }

    pub fn run(
        self,
        cx: &mut TestAppContext,
        test: impl FnOnce(&MarkdownReviewerHarness, &mut TestAppContext) -> Result<(), Box<dyn Error>>,
    ) {
        let harness = self.build(cx);
        run_test(cx, |cx| test(&harness, cx));
    }

    pub fn build(self, cx: &mut TestAppContext) -> MarkdownReviewerHarness {
        cx.update(MarkdownReviewer::bind_keys);
        let Self {
            document,
            theme,
            options,
            comments,
        } = self;
        let window = cx.add_window(|_, _| MarkdownReviewer::with_options(document, theme, options));
        let harness = MarkdownReviewerHarness { window };
        for body in comments {
            harness
                .add_comment(cx, body)
                .expect("seed Markdown review comment");
        }
        draw_window(cx, *window).expect("draw Markdown reviewer");
        harness
    }
}

pub struct MarkdownReviewerHarness {
    window: WindowHandle<MarkdownReviewer>,
}

impl MarkdownReviewerHarness {
    pub fn dispatch_command(
        &self,
        cx: &mut TestAppContext,
        command: impl Into<MarkdownReviewCommand>,
    ) -> Result<bool, Box<dyn Error>> {
        let command = command.into();
        let handled = self.window.update(cx, |reviewer, window, cx| {
            reviewer.handle_command(command, window, cx)
        })??;
        draw_window(cx, *self.window)?;
        Ok(handled)
    }

    pub fn add_comment(
        &self,
        cx: &mut TestAppContext,
        body: impl Into<String>,
    ) -> Result<u64, Box<dyn Error>> {
        let body = body.into();
        if body.trim().is_empty() {
            return Err("cannot seed a blank Markdown comment".into());
        }
        if !self.dispatch_command(cx, ReviewCommand::BeginComment)? {
            return Err("cannot begin Markdown comment".into());
        }
        self.window
            .update(cx, |reviewer, _, _| -> Result<(), Box<dyn Error>> {
                reviewer
                    .session_mut()
                    .draft_mut()
                    .ok_or("missing draft")?
                    .set_body(body);
                Ok(())
            })??;
        self.dispatch_command(cx, ReviewCommand::SubmitComment)?;
        self.review(cx)
            .comments()
            .last()
            .map(|comment| comment.id)
            .ok_or_else(|| "missing submitted comment".into())
    }

    pub fn read<T>(
        &self,
        cx: &TestAppContext,
        read: impl FnOnce(&MarkdownReviewer, &App) -> T,
    ) -> T {
        self.window
            .read_with(cx, read)
            .expect("read Markdown reviewer")
    }

    #[must_use]
    pub fn review(&self, cx: &TestAppContext) -> MarkdownReview {
        self.read(cx, |reviewer, _| reviewer.review().clone())
    }

    #[must_use]
    pub fn comment_bodies(&self, cx: &TestAppContext) -> Vec<String> {
        self.review(cx)
            .comments()
            .iter()
            .map(|comment| comment.body.clone())
            .collect()
    }

    #[must_use]
    pub fn selected_target(&self, cx: &TestAppContext) -> Option<MarkdownTargetId> {
        self.read(cx, |reviewer, _| reviewer.session().selected_target())
    }

    pub fn click(
        &self,
        cx: &mut TestAppContext,
        selector: &'static str,
    ) -> Result<(), Box<dyn Error>> {
        click_element(cx, *self.window, selector)
    }
}

fn run_test(
    cx: &mut TestAppContext,
    test: impl FnOnce(&mut TestAppContext) -> Result<(), Box<dyn Error>>,
) {
    if let Err(error) = test(cx) {
        panic!("{error}");
    }
}

fn draw_window(cx: &mut TestAppContext, window: AnyWindowHandle) -> Result<(), Box<dyn Error>> {
    cx.update_window(window, |_, window, cx| window.draw(cx).clear(cx))?;
    cx.run_until_parked();
    Ok(())
}

fn click_element(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    selector: &'static str,
) -> Result<(), Box<dyn Error>> {
    draw_window(cx, window)?;
    let mut visual = VisualTestContext::from_window(window, cx);
    let bounds = visual
        .debug_bounds(selector)
        .ok_or_else(|| format!("missing {selector}"))?;
    visual.simulate_click(bounds.center(), Modifiers::default());
    draw_window(cx, window)
}

fn press_key(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    key: &str,
) -> Result<(), Box<dyn Error>> {
    draw_window(cx, window)?;
    let keystroke = Keystroke::parse(key)?;
    cx.update_window(window, |_, window, cx| {
        window.dispatch_event(
            KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(KeyUpEvent { keystroke }.to_platform_input(), cx);
    })?;
    draw_window(cx, window)
}
