use clankerdiff_ratatui::{
    highlighted_line,
    syntax::{FontStyle, HighlightId, HighlightSpan, Rgba, SyntaxStyle, SyntaxTheme},
};
use ratatui::style::{Color, Modifier, Style};
use std::{collections::BTreeMap, error::Error, ops::Range};

#[test]
fn highlights_patch_the_base_style_and_keep_utf8_gaps() -> Result<(), Box<dyn Error>> {
    let base = Style::new().bg(Color::Blue).add_modifier(Modifier::DIM);
    let line = highlighted_line("aéz", [keyword(1..3)?], &bold_keywords(), base);
    assert_eq!(line.to_string(), "aéz");
    assert_eq!(line.spans[1].content, "é");
    assert_eq!(line.spans[1].style.bg, Some(Color::Blue));
    assert!(line.spans[1].style.add_modifier.contains(Modifier::BOLD | Modifier::DIM));
    assert!(!line.spans[0].style.add_modifier.contains(Modifier::BOLD));
    Ok(())
}

#[test]
fn spans_the_theme_does_not_style_keep_the_base_style() -> Result<(), Box<dyn Error>> {
    let line = highlighted_line("ab", [keyword(0..2)?], &SyntaxTheme::new(BTreeMap::new()), Style::new());
    assert_eq!(line.to_string(), "ab");
    assert!(line.spans.iter().all(|span| span.style == Style::new()));
    Ok(())
}

#[test]
fn spans_that_split_a_character_are_ignored() -> Result<(), Box<dyn Error>> {
    let line = highlighted_line("éx", [keyword(0..1)?], &bold_keywords(), Style::new());
    assert_eq!(line.to_string(), "éx");
    assert!(line.spans.iter().all(|span| !span.style.add_modifier.contains(Modifier::BOLD)));
    Ok(())
}

#[test]
fn spans_beyond_the_source_are_ignored() -> Result<(), Box<dyn Error>> {
    let line = highlighted_line("ab", [keyword(0..9)?, keyword(1..1)?], &bold_keywords(), Style::new());
    assert_eq!(line.to_string(), "ab");
    assert!(line.spans.iter().all(|span| !span.style.add_modifier.contains(Modifier::BOLD)));
    Ok(())
}

fn keyword(range: Range<usize>) -> Result<HighlightSpan, Box<dyn Error>> {
    Ok(HighlightSpan { range, highlight: HighlightId::from_tag("k").ok_or("keyword highlight")? })
}

fn bold_keywords() -> SyntaxTheme {
    SyntaxTheme::new(BTreeMap::from([(
        "keyword".to_owned(),
        SyntaxStyle {
            foreground: Rgba::new(1, 2, 3, 255),
            font_style: FontStyle { bold: true, italic: false, underline: false },
        },
    )]))
}
