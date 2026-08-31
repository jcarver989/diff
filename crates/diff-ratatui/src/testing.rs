//! Reusable test and benchmark support for the rendered diff-review widget.
//!
//! Enable the `test-support` feature from integration tests and benchmarks. The
//! harness owns a real Ratatui terminal and the production review state so input,
//! rendering, terminal diffing, and syntax caches are all exercised together.

use crate::{DiffReviewInput, DiffReviewState, DiffReviewWidget};
#[cfg(feature = "markdown-review")]
use crate::{MarkdownReviewInput, MarkdownReviewState, MarkdownReviewWidget};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use diff_core::{DiffDocument, DiffSnapshot};
use diff_syntax::HighlightStats;
use ratatui::{
    Terminal,
    backend::{Backend, ClearType, TestBackend, WindowSize},
    buffer::{Buffer, Cell},
    layout::{Position, Size},
};
use std::{convert::Infallible, sync::Arc};

/// Terminal operations emitted by one or more frames.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BackendStats {
    pub draws: u64,
    pub cells_drawn: u64,
}

/// A [`TestBackend`] that counts the terminal cells Ratatui actually emits.
#[derive(Debug)]
pub struct CountingBackend {
    inner: TestBackend,
    stats: BackendStats,
}

impl CountingBackend {
    #[must_use]
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            inner: TestBackend::new(width, height),
            stats: BackendStats::default(),
        }
    }

    pub fn take_stats(&mut self) -> BackendStats {
        std::mem::take(&mut self.stats)
    }

    #[must_use]
    pub const fn buffer(&self) -> &Buffer {
        self.inner.buffer()
    }
}

impl Backend for CountingBackend {
    type Error = Infallible;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        self.stats.draws += 1;
        let mut cells = 0;
        let result = self.inner.draw(content.inspect(|_| cells += 1));
        self.stats.cells_drawn += cells;
        result
    }

    fn append_lines(&mut self, lines: u16) -> Result<(), Self::Error> {
        self.inner.append_lines(lines)
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<T: Into<Position>>(&mut self, position: T) -> Result<(), Self::Error> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> Result<Size, Self::Error> {
        self.inner.size()
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.flush()
    }
}

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
/// use diff_core::testing::DocumentBuilder;
/// use diff_ratatui::testing::ReviewHarnessBuilder;
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
    pub snapshot: Option<DiffSnapshot>,
    pub width: u16,
    pub height: u16,
}

impl Default for ReviewHarnessBuilder {
    fn default() -> Self {
        Self {
            document: Arc::new(DiffDocument::empty()),
            snapshot: None,
            width: 80,
            height: 24,
        }
    }
}

impl ReviewHarnessBuilder {
    #[must_use]
    pub fn from_snapshot(snapshot: DiffSnapshot) -> Self {
        Self {
            snapshot: Some(snapshot),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn dimensions(mut self, width: u16, height: u16) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// # Panics
    ///
    /// Panics if Ratatui cannot initialize the in-memory test terminal.
    #[must_use]
    pub fn build(self) -> ReviewHarness {
        let state = self.snapshot.map_or_else(
            || DiffReviewState::new(self.document),
            DiffReviewState::from_snapshot,
        );
        ReviewHarness {
            terminal: Terminal::new(CountingBackend::new(self.width, self.height))
                .expect("infallible test terminal"),
            state,
        }
    }
}

/// High-level integration harness around the real review state, widget, and terminal.
pub struct ReviewHarness {
    terminal: Terminal<CountingBackend>,
    state: DiffReviewState,
}

impl ReviewHarness {
    #[must_use]
    pub fn new(document: Arc<DiffDocument>, width: u16, height: u16) -> Self {
        ReviewHarnessBuilder {
            document,
            width,
            height,
            ..ReviewHarnessBuilder::default()
        }
        .build()
    }

    #[must_use]
    pub fn from_snapshot(snapshot: DiffSnapshot, width: u16, height: u16) -> Self {
        ReviewHarnessBuilder::from_snapshot(snapshot)
            .dimensions(width, height)
            .build()
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
                frame.render_stateful_widget(
                    DiffReviewWidget::new(),
                    frame.area(),
                    &mut self.state,
                );
                if let Some(position) = self.state.cursor_position() {
                    frame.set_cursor_position(position);
                }
            })
            .expect("infallible test draw");
        let after = self.state.highlight_stats();
        FrameStats::new(self.terminal.backend_mut().take_stats(), before, after)
    }

    pub fn input(&mut self, input: DiffReviewInput) {
        let _ = self.state.handle_input(input);
    }

    pub fn press(&mut self, code: KeyCode) {
        self.input(key(code));
    }

    pub fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            self.press(KeyCode::Char(character));
        }
    }

    pub fn input_and_draw(&mut self, input: DiffReviewInput) -> FrameStats {
        self.input(input);
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

    /// Asserts against rendered output and includes the complete buffer on failure.
    ///
    /// # Panics
    ///
    /// Panics if the rendered buffer does not contain `expected`.
    pub fn assert_contains(&self, expected: &str) {
        let rendered = self.text();
        assert!(
            rendered.contains(expected),
            "rendered output did not contain {expected:?}:\n{rendered}"
        );
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

/// Renders a Markdown review state and returns its visible text.
///
/// # Panics
///
/// Panics if the in-memory test terminal cannot initialize or draw.
#[cfg(feature = "markdown-review")]
pub fn render_markdown_review(state: &mut MarkdownReviewState, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    terminal
        .draw(|frame| {
            frame.render_stateful_widget(MarkdownReviewWidget::new(), frame.area(), state);
            if let Some(position) = state.cursor_position() {
                frame.set_cursor_position(position);
            }
        })
        .expect("draw Markdown review widget");
    buffer_text(terminal.backend().buffer())
}

/// Converts a rendered buffer to fixed-width text rows without snapshot files.
#[must_use]
pub fn buffer_text(buffer: &Buffer) -> String {
    (buffer.area.top()..buffer.area.bottom())
        .map(|row| buffer_row_text(buffer, row))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Returns one rendered row from a buffer.
#[must_use]
pub fn buffer_row_text(buffer: &Buffer, row: u16) -> String {
    (buffer.area.left()..buffer.area.right())
        .filter_map(|column| buffer.cell((column, row)))
        .map(Cell::symbol)
        .collect()
}

#[must_use]
pub fn key(code: KeyCode) -> DiffReviewInput {
    DiffReviewInput::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

#[must_use]
pub fn key_with(code: KeyCode, modifiers: KeyModifiers) -> DiffReviewInput {
    DiffReviewInput::Key(KeyEvent::new(code, modifiers))
}

/// Types text through the diff review input boundary.
pub fn type_review_text(state: &mut DiffReviewState, text: &str) {
    for character in text.chars() {
        let _ = state.handle_input(key(KeyCode::Char(character)));
    }
}

/// Builds a Markdown review key input.
#[cfg(feature = "markdown-review")]
#[must_use]
pub fn markdown_key(code: KeyCode) -> MarkdownReviewInput {
    MarkdownReviewInput::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

/// Builds a Markdown review mouse input.
#[cfg(feature = "markdown-review")]
#[must_use]
pub fn markdown_mouse(kind: MouseEventKind, column: u16, row: u16) -> MarkdownReviewInput {
    MarkdownReviewInput::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

/// Types text through the Markdown review input boundary.
#[cfg(feature = "markdown-review")]
pub fn type_markdown_text(state: &mut MarkdownReviewState, text: &str) {
    for character in text.chars() {
        let _ = state.handle_input(markdown_key(KeyCode::Char(character)));
    }
}

/// A mouse event at a terminal cell, for exercising pointer hit tests.
#[must_use]
pub fn mouse(kind: MouseEventKind, column: u16, row: u16) -> DiffReviewInput {
    DiffReviewInput::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}
