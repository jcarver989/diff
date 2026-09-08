#![cfg(feature = "test-support")]

use clankerdiff_ratatui::testing::{BackendStats, CountingBackend};
use ratatui::{Terminal, backend::Backend, widgets::Paragraph};
use std::error::Error;

#[test]
fn counts_emitted_cells_and_resets_stats() -> Result<(), Box<dyn Error>> {
    let mut terminal = Terminal::new(CountingBackend::new(8, 2))?;
    terminal.draw(|frame| frame.render_widget(Paragraph::new("hello"), frame.area()))?;

    assert_eq!(
        terminal.backend_mut().take_stats(),
        BackendStats {
            draws: 1,
            cells_drawn: 5,
        }
    );
    assert_eq!(terminal.backend_mut().take_stats(), BackendStats::default());
    assert_eq!(terminal.backend().buffer()[(0, 0)].symbol(), "h");

    terminal.draw(|frame| frame.render_widget(Paragraph::new("hello"), frame.area()))?;
    assert_eq!(terminal.backend_mut().take_stats().cells_drawn, 0);
    Ok(())
}

#[test]
fn delegates_cursor_size_and_clear() -> Result<(), Box<dyn Error>> {
    let mut terminal = Terminal::new(CountingBackend::new(8, 2))?;
    terminal.draw(|frame| frame.render_widget(Paragraph::new("hello"), frame.area()))?;
    let backend = terminal.backend_mut();

    backend.set_cursor_position((3, 1))?;
    assert_eq!(backend.get_cursor_position()?, (3, 1).into());
    assert_eq!(backend.size()?.width, 8);
    assert_eq!(backend.size()?.height, 2);
    backend.clear()?;
    assert_eq!(backend.buffer()[(0, 0)].symbol(), " ");
    Ok(())
}
