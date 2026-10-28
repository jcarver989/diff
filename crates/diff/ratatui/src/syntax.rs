//! Conversion of renderer-neutral syntax spans to Ratatui text.

use crate::color::native_color;
pub use clankerdiff_syntax::{
    CacheConfig, CacheKey, CacheUsage, DocumentHighlights, Fingerprint, FontStyle, HighlightSpan, HighlightStats,
    LanguageHint, Rgba, SyntaxError, SyntaxHighlighter, SyntaxStream, SyntaxStreamUpdate, SyntaxStyle, SyntaxTheme,
    SyntaxWorkStats, ThemedHighlighter, empty_spans, resolve_language,
};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::ops::Range;

/// Converts ordered UTF-8 byte spans into a fully-owned Ratatui line.
///
/// Unhighlighted gaps retain `base`. Highlight styles patch `base`, so caller
/// backgrounds and modifiers are preserved rather than replaced.
#[must_use]
pub fn highlighted_line(source: &str, spans: &[HighlightSpan], base: Style) -> Line<'static> {
    let spans = styled_spans(source, spans, &[], base);
    Line::from(spans.into_iter().map(|span| Span::styled(span.content.into_owned(), span.style)).collect::<Vec<_>>())
}

pub(crate) fn styled_spans<'a>(
    source: &'a str,
    highlights: &[HighlightSpan],
    overlays: &[(Range<usize>, Color)],
    base: Style,
) -> Vec<Span<'a>> {
    let valid = |range: &Range<usize>| {
        range.start < range.end
            && range.end <= source.len()
            && source.is_char_boundary(range.start)
            && source.is_char_boundary(range.end)
    };
    let highlights: Vec<&HighlightSpan> = highlights.iter().filter(|highlight| valid(&highlight.range)).collect();
    let overlays: Vec<&(Range<usize>, Color)> = overlays.iter().filter(|(range, _)| valid(range)).collect();
    let mut cuts: Vec<usize> = highlights
        .iter()
        .map(|highlight| &highlight.range)
        .chain(overlays.iter().map(|(range, _)| range))
        .flat_map(|range| [range.start, range.end])
        .chain([0, source.len()])
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    let mut highlights = highlights.into_iter().peekable();
    let mut spans: Vec<Span<'a>> = cuts
        .windows(2)
        .map(|pair| {
            let (start, end) = (pair[0], pair[1]);
            let base =
                overlays.iter().find(|(range, _)| range.contains(&start)).map_or(base, |(_, tint)| base.bg(*tint));
            while highlights.next_if(|highlight| highlight.range.end <= start).is_some() {}
            let style = highlights
                .peek()
                .filter(|highlight| highlight.range.start <= start)
                .map_or(base, |highlight| base.patch(syntax_style(highlight, base)));
            Span::styled(&source[start..end], style)
        })
        .collect();
    if spans.is_empty() {
        spans.push(Span::styled(source, base));
    }
    spans
}

fn syntax_style(highlight: &HighlightSpan, base: Style) -> Style {
    let mut modifiers = Modifier::empty();
    modifiers.set(Modifier::BOLD, highlight.font_style.bold);
    modifiers.set(Modifier::ITALIC, highlight.font_style.italic);
    modifiers.set(Modifier::UNDERLINED, highlight.font_style.underline);
    Style::new().fg(native_color(highlight.foreground, base.bg.unwrap_or(Color::Black))).add_modifier(modifiers)
}
