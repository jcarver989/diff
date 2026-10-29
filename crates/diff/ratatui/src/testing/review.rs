//! Diff review harness, input builders, and a one-shot renderer.

use super::{BackendStats, CountingBackend, buffer_row_text, buffer_text, key, mouse};
use crate::{
    DiffReviewEvent, DiffReviewState, DiffReviewWidget, InputOutcome, KeyCode, MouseButton, MouseEventKind, ReviewInput,
};
use clankerdiff_core::{DiffDocument, Lookup, LookupAnswer, LookupReply, ReviewCapabilities, testing::DocumentBuilder};
use clankerdiff_syntax::HighlightStats;
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::{Position, Rect},
};
use std::sync::Arc;

/// Deterministic work performed while drawing one frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FrameStats {
    pub backend: BackendStats,
    pub highlight_calls: u64,
    pub highlight_hits: u64,
    pub highlight_misses: u64,
    pub highlighted_bytes: usize,
}

/// Struct-update-friendly configuration for a [`ReviewHarness`].
///
/// ```
/// use clankerdiff_core::testing::DocumentBuilder;
/// use clankerdiff_ratatui::testing::ReviewHarnessBuilder;
///
/// let mut harness = ReviewHarnessBuilder {
///     document: DocumentBuilder::new().changed("a.rs", "old\n", "new\n").build(),
///     width: 100,
///     ..ReviewHarnessBuilder::default()
/// }
/// .build();
/// harness.draw();
/// assert!(harness.text().contains("a.rs"));
/// ```
pub struct ReviewHarnessBuilder {
    pub document: Arc<DiffDocument>,
    pub width: u16,
    pub height: u16,
    pub capabilities: ReviewCapabilities,
}

impl Default for ReviewHarnessBuilder {
    fn default() -> Self {
        Self {
            document: Arc::new(DiffDocument::empty()),
            width: 80,
            height: 24,
            capabilities: ReviewCapabilities::default(),
        }
    }
}

impl ReviewHarnessBuilder {
    #[must_use]
    pub fn dimensions(mut self, width: u16, height: u16) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    #[must_use]
    pub const fn with_lookups(mut self) -> Self {
        self.capabilities.lookups = true;
        self
    }

    /// # Panics
    ///
    /// Panics if Ratatui cannot initialize the in-memory test terminal.
    #[must_use]
    pub fn build(self) -> ReviewHarness {
        let mut state = DiffReviewState::new(self.document);
        state.set_capabilities(self.capabilities);
        ReviewHarness {
            terminal: Terminal::new(CountingBackend::new(self.width, self.height)).expect("infallible test terminal"),
            state,
            lookups: Vec::new(),
        }
    }
}

/// High-level integration harness around the real review state, widget, and terminal.
pub struct ReviewHarness {
    terminal: Terminal<CountingBackend>,
    state: DiffReviewState,
    lookups: Vec<Lookup>,
}

impl ReviewHarness {
    #[must_use]
    pub fn new(document: Arc<DiffDocument>, width: u16, height: u16) -> Self {
        ReviewHarnessBuilder { document, width, height, ..ReviewHarnessBuilder::default() }.build()
    }

    /// Draws one frame and returns deterministic rendering work statistics.
    ///
    /// # Panics
    ///
    /// Panics if the in-memory test terminal cannot draw the frame.
    pub fn draw(&mut self) -> FrameStats {
        let before = self.state.highlight_stats();
        self.terminal
            .draw(|frame| {
                frame.render_stateful_widget(DiffReviewWidget::new(), frame.area(), &mut self.state);
                if let Some(position) = self.state.cursor_position() {
                    frame.set_cursor_position(position);
                }
            })
            .expect("infallible test draw");
        let after = self.state.highlight_stats();
        FrameStats::new(self.terminal.backend_mut().take_stats(), before, after)
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.terminal.resize(Rect::new(0, 0, width, height)).expect("infallible test resize");
        self.state.mark_dirty();
    }

    pub fn input(&mut self, input: ReviewInput) -> InputOutcome<DiffReviewEvent> {
        let outcome = self.state.handle_input(input);
        self.lookups.extend(self.state.take_lookups());
        outcome
    }

    pub fn click_at(&mut self, position: Position) -> InputOutcome<DiffReviewEvent> {
        self.input(mouse(MouseEventKind::Down(MouseButton::Left), position.x, position.y))
    }

    pub fn click(&mut self, text: &str) -> Result<InputOutcome<DiffReviewEvent>, String> {
        let position = self.find(text)?;
        Ok(self.click_at(position))
    }

    pub fn hover(&mut self, text: &str) -> Result<InputOutcome<DiffReviewEvent>, String> {
        let position = self.find(text)?;
        Ok(self.input(mouse(MouseEventKind::Moved, position.x, position.y)))
    }

    #[must_use]
    pub fn last_lookup(&self) -> Option<Lookup> {
        self.lookups.last().cloned()
    }

    pub fn answer_lookup(&mut self, lookup: Lookup, result: Result<LookupAnswer, String>) {
        self.state.lookup_finished(LookupReply::new(lookup, result));
        self.lookups.extend(self.state.take_lookups());
    }

    pub fn press(&mut self, code: KeyCode) {
        self.input(key(code));
    }

    pub fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            self.press(KeyCode::Char(character));
        }
    }

    pub fn input_and_draw(&mut self, input: ReviewInput) -> FrameStats {
        let _ = self.input(input);
        self.draw()
    }

    #[must_use]
    pub const fn state(&self) -> &DiffReviewState {
        &self.state
    }

    pub const fn state_mut(&mut self) -> &mut DiffReviewState {
        &mut self.state
    }

    #[must_use]
    pub fn buffer(&self) -> &Buffer {
        self.terminal.backend().buffer()
    }

    /// Returns all rendered rows, preserving spaces within each fixed-width row.
    #[must_use]
    pub fn text(&self) -> String {
        buffer_text(self.buffer())
    }

    #[must_use]
    pub fn row_text(&self, row: u16) -> String {
        buffer_row_text(self.buffer(), row)
    }

    #[must_use]
    pub fn row_containing(&self, text: &str) -> Option<u16> {
        (0..self.buffer().area.height).find(|&row| self.row_text(row).contains(text))
    }

    #[must_use]
    pub fn position_of(&self, text: &str) -> Option<Position> {
        (0..self.buffer().area.height).find_map(|row| {
            let line = self.row_text(row);
            let column = line[..line.find(text)?].chars().count();
            Some(Position::new(u16::try_from(column).ok()?, row))
        })
    }

    #[must_use]
    pub fn position_in(&self, line: &str, text: &str) -> Option<Position> {
        let start = self.position_of(line)?;
        let offset = u16::try_from(line[..line.find(text)?].chars().count()).ok()?;
        Some(Position::new(start.x + offset, start.y))
    }

    fn find(&self, text: &str) -> Result<Position, String> {
        self.position_of(text).ok_or_else(|| format!("{text:?} is not shown:\n{}", self.text()))
    }

    /// Asserts against rendered output and includes the complete buffer on failure.
    ///
    /// # Panics
    ///
    /// Panics if the rendered buffer does not contain `expected`.
    pub fn assert_contains(&self, expected: &str) {
        let rendered = self.text();
        assert!(rendered.contains(expected), "rendered output did not contain {expected:?}:\n{rendered}");
    }

    /// Asserts a rendered row after ignoring incidental trailing terminal spaces.
    ///
    /// # Panics
    ///
    /// Panics if the rendered row differs from `expected`.
    pub fn assert_row(&self, row: u16, expected: &str) {
        let actual = self.row_text(row);
        assert_eq!(
            actual.trim_end(),
            expected.trim_end(),
            "rendered row {row} differed; full buffer:\n{}",
            self.text()
        );
    }
}

impl FrameStats {
    fn new(backend: BackendStats, before: HighlightStats, after: HighlightStats) -> Self {
        Self {
            backend,
            highlight_calls: after.calls.saturating_sub(before.calls),
            highlight_hits: after.hits.saturating_sub(before.hits),
            highlight_misses: after.misses.saturating_sub(before.misses),
            highlighted_bytes: after.bytes.saturating_sub(before.bytes),
        }
    }
}

/// Renders a review state through the production widget and returns its visible text.
///
/// # Panics
///
/// Panics if the in-memory test terminal cannot initialize or draw.
pub fn render_review(state: &mut DiffReviewState, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    terminal
        .draw(|frame| {
            frame.render_stateful_widget(DiffReviewWidget::new(), frame.area(), state);
            if let Some(position) = state.cursor_position() {
                frame.set_cursor_position(position);
            }
        })
        .expect("draw review widget");
    buffer_text(terminal.backend().buffer())
}

/// Types text through the diff review input boundary.
pub fn type_review_text(state: &mut DiffReviewState, text: &str) {
    for character in text.chars() {
        let _ = state.handle_input(key(KeyCode::Char(character)));
    }
}

#[must_use]
pub fn caller_and_callee_document() -> Arc<DiffDocument> {
    DocumentBuilder::new()
        .changed("src/lib.rs", "", "pub fn double(value: u32) -> u32 {\n    value * 2\n}\n")
        .changed("src/main.rs", "fn main() {}\n", "fn main() {\n    double(1);\n}\n")
        .build()
}
