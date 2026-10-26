use clankerdiff_ratatui::{MarkdownReview, MarkdownReviewEvent};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    DefaultTerminal,
    layout::{Constraint, Layout},
    widgets::Paragraph,
};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut terminal = ratatui::init();
    let result = run(&mut terminal);
    ratatui::restore();
    if let Some(review) = result? {
        println!("{review}");
    }
    Ok(())
}

fn run(terminal: &mut DefaultTerminal) -> Result<Option<String>, Box<dyn Error>> {
    let mut review = MarkdownReview::builder()
        .markdown(
            "# Embedded review\n\nThe host owns the surrounding UI and handles emitted actions.",
        )
        .embedded()
        .build();
    let mut status = String::from("c: comment | a: approve | r: request changes | q: close host");

    loop {
        terminal.draw(|frame| {
            let [header, body, footer] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .areas(frame.area());
            frame.render_widget(Paragraph::new("Host application"), header);
            review.render(frame, body);
            frame.render_widget(Paragraph::new(status.as_str()), footer);
        })?;

        let event = event::read()?;
        let outcome = review.handle_crossterm_event(&event)?;
        if !outcome.is_consumed()
            && matches!(event, Event::Key(key) if key.kind != KeyEventKind::Release && key.code == KeyCode::Char('q'))
        {
            return Ok(None);
        }
        match outcome.into_event() {
            Some(MarkdownReviewEvent::Submit(submission)) => return Ok(Some(submission.formatted)),
            Some(MarkdownReviewEvent::Cancel) => return Ok(None),
            Some(MarkdownReviewEvent::CopyFormatted(text)) => {
                status = format!("Copy requested: {text}");
            }
            None => {}
        }
    }
}
