#![allow(dead_code, missing_docs)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use diff_core::{
    DiffDocument, FileDiff, FileStatus, HighlightStats, Hunk, PatchLine, RepoPath, StageState,
};
use diff_ratatui::{DiffReviewInput, DiffReviewState, DiffReviewWidget};
use ratatui::{
    Terminal,
    backend::{Backend, ClearType, TestBackend, WindowSize},
    buffer::{Buffer, Cell},
    layout::{Position, Size},
};
use std::{convert::Infallible, sync::Arc};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BackendStats {
    pub draws: u64,
    pub cells_drawn: u64,
}

#[derive(Debug)]
pub struct CountingBackend {
    inner: TestBackend,
    stats: BackendStats,
}

impl CountingBackend {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            inner: TestBackend::new(width, height),
            stats: BackendStats::default(),
        }
    }

    pub fn take_stats(&mut self) -> BackendStats {
        std::mem::take(&mut self.stats)
    }

    pub fn buffer(&self) -> &Buffer {
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FrameStats {
    pub backend: BackendStats,
    pub highlight_calls: u64,
    pub highlight_hits: u64,
    pub highlight_misses: u64,
    pub highlighted_bytes: usize,
}

pub struct ReviewHarness {
    terminal: Terminal<CountingBackend>,
    state: DiffReviewState,
}

impl ReviewHarness {
    pub fn new(document: Arc<DiffDocument>, width: u16, height: u16) -> Self {
        Self {
            terminal: Terminal::new(CountingBackend::new(width, height))
                .expect("infallible test terminal"),
            state: DiffReviewState::new(document),
        }
    }

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

    pub fn input_and_draw(&mut self, input: DiffReviewInput) -> FrameStats {
        self.input(input);
        self.draw()
    }

    pub fn state(&self) -> &DiffReviewState {
        &self.state
    }

    pub fn buffer(&self) -> &Buffer {
        self.terminal.backend().buffer()
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

pub fn key(code: KeyCode) -> DiffReviewInput {
    DiffReviewInput::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

pub fn large_document(rows: usize) -> Arc<DiffDocument> {
    let path = RepoPath::new("src/large.rs").expect("valid fixture path");
    let lines = (1..=rows)
        .map(|line| PatchLine::added(format!("let value_{line} = {line};"), line))
        .collect();
    Arc::new(DiffDocument {
        repo_root: "/repo".to_owned(),
        files: vec![added_file(path, rows, lines)],
    })
}

pub fn many_file_document(files: usize, rows_per_file: usize) -> Arc<DiffDocument> {
    let files = (0..files)
        .map(|file_index| {
            let path =
                RepoPath::new(format!("src/file_{file_index}.rs")).expect("valid fixture path");
            let lines = (1..=rows_per_file)
                .map(|line| {
                    PatchLine::added(
                        format!("let file_{file_index}_value_{line} = {line};"),
                        line,
                    )
                })
                .collect();
            added_file(path, rows_per_file, lines)
        })
        .collect();
    Arc::new(DiffDocument {
        repo_root: "/repo".to_owned(),
        files,
    })
}

fn added_file(path: RepoPath, rows: usize, lines: Vec<PatchLine>) -> FileDiff {
    FileDiff {
        old_path: None,
        path,
        status: FileStatus::Added,
        staged: StageState::Unstaged,
        hunks: vec![Hunk {
            header: format!("@@ -0,0 +1,{rows} @@"),
            function_context: None,
            old_start: 0,
            old_count: 0,
            new_start: 1,
            new_count: rows,
            lines,
        }],
        binary: false,
        mode: None,
        no_newline_at_end: false,
    }
}
