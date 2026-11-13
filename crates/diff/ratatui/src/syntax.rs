//! Conversion of renderer-neutral syntax spans to Ratatui text.

use crate::color::native_color;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
pub use rouge_syntax::{
    DocumentHighlights, FontStyle, HighlightId, HighlightLine, HighlightSpan, LanguageHint, Rgba, SourceSyntax,
    SyntaxError, SyntaxStream, SyntaxStreamUpdate, SyntaxStyle, SyntaxTheme, SyntaxWorkStats, analyze,
    resolve_language,
};
use std::ops::Range;

/// Converts ordered UTF-8 byte spans into a fully-owned Ratatui line.
///
/// Unhighlighted gaps retain `base`. Highlight styles patch `base`, so caller
/// backgrounds and modifiers are preserved rather than replaced.
#[must_use]
pub fn highlighted_line(
    source: &str,
    spans: impl IntoIterator<Item = HighlightSpan>,
    theme: &SyntaxTheme,
    base: Style,
) -> Line<'static> {
    let spans = styled_spans(source, spans, theme, &[], base);
    Line::from(spans.into_iter().map(|span| Span::styled(span.content.into_owned(), span.style)).collect::<Vec<_>>())
}

pub(crate) fn styled_spans<'a>(
    source: &'a str,
    highlights: impl IntoIterator<Item = HighlightSpan>,
    theme: &SyntaxTheme,
    overlays: &[(Range<usize>, Style)],
    base: Style,
) -> Vec<Span<'a>> {
    let valid = |range: &Range<usize>| {
        range.start < range.end
            && range.end <= source.len()
            && source.is_char_boundary(range.start)
            && source.is_char_boundary(range.end)
    };
    let overlays: Vec<&(Range<usize>, Style)> = overlays.iter().filter(|(range, _)| valid(range)).collect();
    let mut cuts: Vec<usize> = overlays.iter().flat_map(|(range, _)| [range.start, range.end]).collect();
    cuts.sort_unstable();
    cuts.dedup();
    let mut spans: Vec<Span<'a>> = Vec::new();
    let mut emit = |from: usize, to: usize, syntax: Option<SyntaxStyle>| {
        let mut start = from;
        let first = cuts.partition_point(|&cut| cut <= from);
        for &end in cuts[first..].iter().take_while(|&&cut| cut < to).chain([&to]) {
            let base = overlays
                .iter()
                .filter(|(range, _)| range.contains(&start))
                .fold(base, |style, (_, overlay)| style.patch(*overlay));
            let style = syntax.map_or(base, |syntax| base.patch(syntax_style(syntax, base)));
            spans.push(Span::styled(&source[start..end], style));
            start = end;
        }
    };
    let mut position = 0;
    for highlight in highlights {
        let range = highlight.range;
        if !valid(&range) || range.start < position {
            continue;
        }
        let Some(style) = theme.highlight(highlight.highlight) else {
            continue;
        };
        if position < range.start {
            emit(position, range.start, None);
        }
        emit(range.start, range.end, Some(style));
        position = range.end;
    }
    if position < source.len() {
        emit(position, source.len(), None);
    }
    if spans.is_empty() {
        spans.push(Span::styled(source, base));
    }
    spans
}

fn syntax_style(style: SyntaxStyle, base: Style) -> Style {
    let mut modifiers = Modifier::empty();
    modifiers.set(Modifier::BOLD, style.font_style.bold);
    modifiers.set(Modifier::ITALIC, style.font_style.italic);
    modifiers.set(Modifier::UNDERLINED, style.font_style.underline);
    Style::new().fg(native_color(style.foreground, base.bg.unwrap_or(Color::Black))).add_modifier(modifiers)
}
