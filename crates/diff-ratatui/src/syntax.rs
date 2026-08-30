//! Conversion of renderer-neutral syntax spans to Ratatui text.

use diff_syntax::HighlightSpan;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Converts ordered UTF-8 byte spans into a fully-owned Ratatui line.
///
/// Unhighlighted gaps retain `base`. Highlight styles patch `base`, so caller
/// backgrounds and modifiers are preserved rather than replaced.
#[must_use]
pub fn highlighted_line(source: &str, spans: &[HighlightSpan], base: Style) -> Line<'static> {
    let mut output = Vec::with_capacity(spans.len().saturating_mul(2).saturating_add(1));
    let mut cursor = 0;
    for span in spans {
        let start = span.range.start.max(cursor);
        let end = span.range.end;
        if start >= end
            || end > source.len()
            || !source.is_char_boundary(start)
            || !source.is_char_boundary(end)
        {
            continue;
        }
        if cursor < start {
            output.push(Span::styled(source[cursor..start].to_owned(), base));
        }
        let mut modifiers = Modifier::empty();
        modifiers.set(Modifier::BOLD, span.font_style.bold);
        modifiers.set(Modifier::ITALIC, span.font_style.italic);
        modifiers.set(Modifier::UNDERLINED, span.font_style.underline);
        let syntax = Style::new()
            .fg(Color::Rgb(
                span.foreground.r,
                span.foreground.g,
                span.foreground.b,
            ))
            .add_modifier(modifiers);
        output.push(Span::styled(
            source[start..end].to_owned(),
            base.patch(syntax),
        ));
        cursor = end;
    }
    if cursor < source.len() {
        output.push(Span::styled(source[cursor..].to_owned(), base));
    }
    if output.is_empty() {
        output.push(Span::styled(source.to_owned(), base));
    }
    Line::from(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use diff_theme::{FontStyle, Rgba};

    #[test]
    fn preserves_utf8_gaps_and_base_background() {
        let base = Style::new().bg(Color::Blue).add_modifier(Modifier::DIM);
        let line = highlighted_line(
            "aéz",
            &[HighlightSpan {
                range: 1..3,
                foreground: Rgba::new(1, 2, 3, 255),
                font_style: FontStyle {
                    bold: true,
                    italic: false,
                    underline: false,
                },
            }],
            base,
        );
        assert_eq!(line.to_string(), "aéz");
        assert_eq!(line.spans[1].style.bg, Some(Color::Blue));
        assert!(line.spans[1].style.add_modifier.contains(Modifier::BOLD));
        assert!(line.spans[1].style.add_modifier.contains(Modifier::DIM));
    }
}
