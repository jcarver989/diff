use ratatui::buffer::{Buffer, Cell};

#[must_use]
pub fn buffer_text(buffer: &Buffer) -> String {
    (buffer.area.top()..buffer.area.bottom())
        .map(|row| buffer_row_text(buffer, row))
        .collect::<Vec<_>>()
        .join("\n")
}

#[must_use]
pub fn buffer_row_text(buffer: &Buffer, row: u16) -> String {
    (buffer.area.left()..buffer.area.right())
        .filter_map(|column| buffer.cell((column, row)))
        .map(Cell::symbol)
        .collect()
}
