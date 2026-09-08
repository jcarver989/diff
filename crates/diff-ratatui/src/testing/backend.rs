use ratatui::{
    backend::{Backend, ClearType, TestBackend, WindowSize},
    buffer::{Buffer, Cell},
    layout::{Position, Size},
};
use std::mem;

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
    #[must_use]
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            inner: TestBackend::new(width, height),
            stats: BackendStats::default(),
        }
    }

    pub fn take_stats(&mut self) -> BackendStats {
        mem::take(&mut self.stats)
    }

    #[must_use]
    pub const fn buffer(&self) -> &Buffer {
        self.inner.buffer()
    }
}

impl Backend for CountingBackend {
    type Error = <TestBackend as Backend>::Error;

    fn draw<'a, T>(&mut self, content: T) -> Result<(), Self::Error>
    where
        T: Iterator<Item = (u16, u16, &'a Cell)>,
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
