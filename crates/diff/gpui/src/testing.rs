//! Reusable GPUI integration-test support.
//!
//! The harness opens the production [`DiffViewer`] in a real
//! GPUI test window, records host events, drives input through GPUI, and exposes
//! rendered element bounds. It deliberately does not use image snapshots.

use crate::{
    DiffViewer, DiffViewerEvent, DiffViewerOptions, MarkdownReviewer, MarkdownReviewerOptions,
    ui::{InterfaceSize, testing::ViewHarness, tokens::BASE_SIZE},
};
use clankerdiff_core::{
    DiffDocument, DiffReviewCommand, Lookup, LookupAnswer, LookupReply, Review, ReviewCapabilities, ReviewCommand,
    SourceLocation, ViewMode, testing::DocumentBuilder,
};
use clankerdiff_markdown::{
    MarkdownDocument, MarkdownReview, MarkdownReviewCommand, MarkdownReviewEvent, MarkdownTargetId,
};
use clankerdiff_symbols::DefinitionCard;
use clankerdiff_ui_core::ReviewTheme;
use gpui::{
    App, Bounds, Capslock, Context, ElementInputHandler, Entity, EventEmitter, Focusable, InputHandler, ListOffset,
    Modifiers, ModifiersChangedEvent, MouseMoveEvent, Pixels, Point, Render, TestAppContext, Window, WindowBounds,
    WindowOptions, point, prelude::*, px, size,
};
use gpui_base::input::TextareaState;
use std::{error::Error, ops::Deref, sync::Arc, time::Duration};

/// A high-level integration harness around a rendered GPUI view and the host events it emits.
pub struct Harness<T: 'static, U: 'static>(ViewHarness<T, U>);

pub type DiffViewerHarness = Harness<DiffViewer, DiffViewerEvent>;
pub type MarkdownReviewerHarness = Harness<MarkdownReviewer, MarkdownReviewEvent>;

impl<T, U> Deref for Harness<T, U> {
    type Target = ViewHarness<T, U>;

    fn deref(&self) -> &ViewHarness<T, U> {
        &self.0
    }
}

impl<T: Render + EventEmitter<U>, U: Clone + 'static> Harness<T, U> {
    fn open(cx: &mut TestAppContext, window_options: WindowOptions, view: impl FnOnce() -> T) -> Self {
        Self(ViewHarness::open(cx, window_options, |_, cx| cx.new(|_| view())))
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
    pub interface_size: f32,
    pub capabilities: ReviewCapabilities,
}

impl Default for DiffViewerHarnessBuilder {
    fn default() -> Self {
        Self {
            document: DocumentBuilder::new().changed("src/lib.rs", "old\n", "new\n").build(),
            theme: ReviewTheme::default(),
            options: DiffViewerOptions::default(),
            window_options: WindowOptions::default(),
            comments: Vec::new(),
            interface_size: BASE_SIZE,
            capabilities: ReviewCapabilities::default(),
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
        self.options.view_mode = view_mode;
        self
    }

    #[must_use]
    pub const fn with_lookups(mut self) -> Self {
        self.capabilities.lookups = true;
        self
    }

    #[must_use]
    pub fn with_window_size(mut self, width: f32, height: f32) -> Self {
        self.window_options.window_bounds = Some(window_bounds(width, height));
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
        let Self { document, theme, options, window_options, comments, interface_size, capabilities } = self;
        cx.update(|cx| InterfaceSize::init(interface_size, cx));
        let harness = Harness::open(cx, window_options, move || DiffViewer::with_options(document, theme, options));
        harness.update(cx, |viewer, cx| viewer.set_capabilities(capabilities, cx));
        for body in comments {
            harness.add_comment(cx, body).expect("seed review comment");
        }
        harness.draw(cx);
        harness
    }
}

impl DiffViewerHarness {
    #[must_use]
    pub fn viewer(&self) -> Entity<DiffViewer> {
        self.view().clone()
    }

    pub fn show(&self, cx: &mut TestAppContext, document: Arc<DiffDocument>) {
        self.update(cx, |viewer, cx| viewer.set_document(document, cx));
        self.draw(cx);
    }

    pub fn add_comment(&self, cx: &mut TestAppContext, body: impl Into<String>) -> Result<u64, Box<dyn Error>> {
        let (anchor, text) = self.read(cx, |viewer, _| {
            Ok::<_, Box<dyn Error>>((
                viewer.session().selected_anchor().ok_or("missing selected anchor")?,
                viewer.session().selected_cell().ok_or("missing selected cell")?.text.to_string(),
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
        self.review(cx).comments().map(|comment| comment.body.clone()).collect()
    }

    #[must_use]
    pub fn selected_file(&self, cx: &TestAppContext) -> Option<usize> {
        self.read(cx, |viewer, _| viewer.selected_file())
    }

    #[must_use]
    pub fn selected_row(&self, cx: &TestAppContext) -> Option<usize> {
        self.read(cx, |viewer, _| viewer.session().selected_row())
    }

    #[must_use]
    pub fn draft_body(&self, cx: &TestAppContext) -> Option<String> {
        self.read(cx, |viewer, _| viewer.session().draft().map(|draft| draft.body().to_owned()))
    }

    pub fn dispatch_command(
        &self,
        cx: &mut TestAppContext,
        command: impl Into<DiffReviewCommand>,
    ) -> Result<bool, Box<dyn Error>> {
        let command = command.into();
        let handled = cx.update_window(self.window(), |_, window, cx| {
            self.view().update(cx, |viewer, cx| viewer.handle_command(command, window, cx))
        })?;
        self.draw(cx);
        Ok(handled)
    }

    pub fn with_comment_input<T>(
        &self,
        cx: &mut TestAppContext,
        update: impl FnOnce(&mut dyn InputHandler, &mut Window, &mut App) -> T,
    ) -> Result<T, Box<dyn Error>> {
        self.draw(cx);
        let editor = self.read(cx, |viewer, _| viewer.comment_editor.clone()).ok_or("no comment editor")?;

        let input = editor.read_with(cx, |editor, _| editor.input());
        let bounds = input.read_with(cx, |input, _| input.text_bounds()).ok_or("comment text not painted")?;
        let result = cx.update_window(self.window(), |_, window, cx| {
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
            .read(cx, |viewer, cx| viewer.comment_editor.as_ref().map(|editor| editor.read(cx).input()))
            .ok_or("no comment editor")?;
        let result =
            cx.update_window(self.window(), |_, window, cx| input.update(cx, |input, cx| update(input, window, cx)))?;
        self.draw(cx);
        Ok(result)
    }

    pub fn comment_focused(&self, cx: &mut TestAppContext) -> Result<bool, Box<dyn Error>> {
        self.with_comment_state(cx, |input, window, cx| input.focus_handle(cx).is_focused(window))
    }

    #[must_use]
    pub fn comment_text_bounds(&self, cx: &TestAppContext) -> Option<Bounds<Pixels>> {
        self.read(cx, |viewer, cx| {
            viewer.comment_editor.as_ref().and_then(|editor| editor.read(cx).input().read(cx).text_bounds())
        })
    }

    pub fn settle(&self, cx: &mut TestAppContext) {
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
        self.draw(cx);
    }

    pub fn select_line(&self, cx: &mut TestAppContext, location: &SourceLocation) -> Result<(), Box<dyn Error>> {
        let row = self
            .read(cx, |viewer, _| viewer.presentation().row_showing_source(location))
            .ok_or("line is not visible")?;
        self.dispatch_command(cx, DiffReviewCommand::SelectRow(row))?
            .then_some(())
            .ok_or_else(|| "line can't be selected".into())
    }

    #[must_use]
    pub fn selected_line(&self, cx: &TestAppContext) -> Option<SourceLocation> {
        self.read(cx, |viewer, _| viewer.session().selected_source_line())
    }

    pub fn hold_secondary(&self, cx: &mut TestAppContext, held: bool) {
        let modifiers = if held { Modifiers::secondary_key() } else { Modifiers::default() };
        self.simulate_event(cx, ModifiersChangedEvent { modifiers, capslock: Capslock::default() });
        self.draw(cx);
    }

    #[must_use]
    pub fn definition_card_name(&self, cx: &TestAppContext) -> Option<String> {
        self.definition_card(cx).map(|card| card.name)
    }

    #[must_use]
    pub fn definition_card(&self, cx: &TestAppContext) -> Option<DefinitionCard> {
        self.read(cx, |viewer, _| viewer.names.card().cloned())
    }

    #[must_use]
    pub fn last_lookup(&self, cx: &TestAppContext) -> Option<Lookup> {
        self.events(cx).into_iter().rev().find_map(|event| match event {
            DiffViewerEvent::Lookup(lookup) => Some(lookup),
            _ => None,
        })
    }

    pub fn answer_lookup(&self, cx: &mut TestAppContext, lookup: Lookup, result: Result<LookupAnswer, String>) {
        self.update(cx, |viewer, cx| viewer.lookup_finished(LookupReply::new(lookup, result), cx));
        self.draw(cx);
    }

    pub fn hover_name(
        &self,
        cx: &mut TestAppContext,
        selector: &'static str,
        name: &str,
    ) -> Result<Point<Pixels>, Box<dyn Error>> {
        self.hold_secondary(cx, true);
        let bounds = self.bounds(cx, selector).ok_or(selector)?;
        let y = bounds.top() + px(6.0);
        let mut x = bounds.left() + px(1.0);
        while x < bounds.right() {
            let position = point(x, y);
            self.simulate_event(
                cx,
                MouseMoveEvent { position, pressed_button: None, modifiers: Modifiers::secondary_key() },
            );
            self.draw(cx);
            if self.definition_card_name(cx).as_deref() == Some(name) {
                return Ok(position);
            }
            x += px(2.0);
        }
        Err(format!("{name} is not under {selector}").into())
    }

    #[must_use]
    pub fn diff_scroll_top(&self, cx: &TestAppContext) -> ListOffset {
        self.read(cx, |viewer, _| viewer.diff_scroll_top())
    }

    pub fn scroll_to_bottom_of_diff(&self, cx: &mut TestAppContext) {
        self.update(cx, DiffViewer::scroll_diff_to_end);
        self.draw(cx);
    }
}

pub struct MarkdownReviewerHarnessBuilder {
    pub source: String,
    pub path: Option<String>,
    pub title: Option<String>,
    pub theme: ReviewTheme,
    pub options: MarkdownReviewerOptions,
    pub window_options: WindowOptions,
    pub comments: Vec<String>,
}

impl Default for MarkdownReviewerHarnessBuilder {
    fn default() -> Self {
        Self {
            source: "# Heading\n\nText".to_owned(),
            path: None,
            title: None,
            theme: ReviewTheme::default(),
            options: MarkdownReviewerOptions::default(),
            window_options: WindowOptions::default(),
            comments: Vec::new(),
        }
    }
}

impl MarkdownReviewerHarnessBuilder {
    #[must_use]
    pub fn with_source(mut self, source: &str) -> Self {
        source.clone_into(&mut self.source);
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, path: Option<&str>, title: Option<&str>) -> Self {
        self.path = path.map(str::to_owned);
        self.title = title.map(str::to_owned);
        self
    }

    #[must_use]
    pub fn with_options(mut self, options: MarkdownReviewerOptions) -> Self {
        self.options = options;
        self
    }

    #[must_use]
    pub fn with_window_size(mut self, width: f32, height: f32) -> Self {
        self.window_options.window_bounds = Some(window_bounds(width, height));
        self
    }

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

    /// Opens the configured reviewer, records its events, and settles the initial frame.
    ///
    /// # Panics
    ///
    /// Panics if GPUI cannot open, update, or draw the test window.
    pub fn build(self, cx: &mut TestAppContext) -> MarkdownReviewerHarness {
        cx.update(MarkdownReviewer::bind_keys);
        let Self { source, path, title, theme, options, window_options, comments } = self;
        let document = Arc::new(MarkdownDocument::parse_with_metadata(path, title, source));
        let harness =
            Harness::open(cx, window_options, move || MarkdownReviewer::with_options(document, theme, options));
        for body in comments {
            harness.add_comment(cx, body).expect("seed Markdown review comment");
        }
        harness.draw(cx);
        harness
    }
}

impl MarkdownReviewerHarness {
    pub fn dispatch_command(
        &self,
        cx: &mut TestAppContext,
        command: impl Into<MarkdownReviewCommand>,
    ) -> Result<bool, Box<dyn Error>> {
        let command = command.into();
        let handled = cx.update_window(self.window(), |_, window, cx| {
            self.view().update(cx, |reviewer, cx| reviewer.handle_command(command, window, cx))
        })??;
        self.draw(cx);
        Ok(handled)
    }

    pub fn add_comment(&self, cx: &mut TestAppContext, body: impl Into<String>) -> Result<u64, Box<dyn Error>> {
        let body = body.into();
        if body.trim().is_empty() {
            return Err("cannot seed a blank Markdown comment".into());
        }
        if !self.dispatch_command(cx, ReviewCommand::BeginComment)? {
            return Err("cannot begin Markdown comment".into());
        }
        self.update(cx, |reviewer, _| {
            reviewer.session_mut().draft_mut().map(|draft| draft.set_body(body)).ok_or("missing draft")
        })?;
        self.dispatch_command(cx, ReviewCommand::SubmitComment)?;
        self.review(cx).comments().last().map(|comment| comment.id).ok_or_else(|| "missing submitted comment".into())
    }

    #[must_use]
    pub fn review(&self, cx: &TestAppContext) -> MarkdownReview {
        self.read(cx, |reviewer, _| reviewer.review().clone())
    }

    #[must_use]
    pub fn comment_bodies(&self, cx: &TestAppContext) -> Vec<String> {
        self.review(cx).comments().iter().map(|comment| comment.body.clone()).collect()
    }

    #[must_use]
    pub fn selected_target(&self, cx: &TestAppContext) -> Option<MarkdownTargetId> {
        self.read(cx, |reviewer, _| reviewer.session().selected_target())
    }
}

fn run_test(cx: &mut TestAppContext, test: impl FnOnce(&mut TestAppContext) -> Result<(), Box<dyn Error>>) {
    if let Err(error) = test(cx) {
        panic!("{error}");
    }
}

fn window_bounds(width: f32, height: f32) -> WindowBounds {
    WindowBounds::Windowed(Bounds::new(point(px(0.0), px(0.0)), size(px(width), px(height))))
}
