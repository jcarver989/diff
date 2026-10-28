use clankerdiff_ratatui::{
    highlighted_line,
    syntax::{FontStyle, HighlightSpan, Rgba},
};
use ratatui::style::{Color, Modifier, Style};
use std::ops::Range;

#[test]
fn highlights_patch_the_base_style_and_keep_utf8_gaps() {
    let base = Style::new().bg(Color::Blue).add_modifier(Modifier::DIM);
    let line = highlighted_line("aéz", &[bold(1..3)], base);
    assert_eq!(line.to_string(), "aéz");
    assert_eq!(line.spans[1].content, "é");
    assert_eq!(line.spans[1].style.bg, Some(Color::Blue));
    assert!(line.spans[1].style.add_modifier.contains(Modifier::BOLD | Modifier::DIM));
    assert!(!line.spans[0].style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn spans_that_split_a_character_are_ignored() {
    let line = highlighted_line("éx", &[bold(0..1)], Style::new());
    assert_eq!(line.to_string(), "éx");
    assert!(line.spans.iter().all(|span| !span.style.add_modifier.contains(Modifier::BOLD)));
}

#[test]
fn spans_beyond_the_source_are_ignored() {
    let line = highlighted_line("ab", &[bold(0..9), bold(1..1)], Style::new());
    assert_eq!(line.to_string(), "ab");
    assert!(line.spans.iter().all(|span| !span.style.add_modifier.contains(Modifier::BOLD)));
}

fn bold(range: Range<usize>) -> HighlightSpan {
    HighlightSpan {
        range,
        foreground: Rgba::new(1, 2, 3, 255),
        font_style: FontStyle { bold: true, italic: false, underline: false },
    }
}
