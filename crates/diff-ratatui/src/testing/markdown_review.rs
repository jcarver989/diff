use super::{buffer_text, key};
use crate::{KeyCode, MarkdownReviewState, MarkdownReviewWidget};
use ratatui::{Terminal, backend::TestBackend};

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

pub fn type_markdown_text(state: &mut MarkdownReviewState, text: &str) {
    for character in text.chars() {
        let _ = state.handle_input(key(KeyCode::Char(character)));
    }
}
