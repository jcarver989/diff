use ratatui::{
    style::Style,
    text::{Line, Span},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy)]
pub(crate) struct FitOptions<'a> {
    pub width: usize,
    pub wrap: bool,
    pub tab_width: usize,
    pub continuation: &'a str,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FitPosition {
    pub byte: usize,
    pub tab_remaining: usize,
}

pub(crate) fn fit_spans(spans: Vec<Span<'_>>, options: FitOptions<'_>) -> Vec<Line<'static>> {
    fit_spans_from(spans, options, FitPosition::default())
        .into_iter()
        .map(|(line, _)| line)
        .collect()
}

pub(crate) fn fit_spans_from(
    spans: Vec<Span<'_>>,
    options: FitOptions<'_>,
    from: FitPosition,
) -> Vec<(Line<'static>, FitPosition)> {
    let text: String = spans.iter().map(|span| span.content.as_ref()).collect();
    if options.width == 0 {
        return vec![(
            Line::default(),
            FitPosition {
                byte: text.len(),
                tab_remaining: 0,
            },
        )];
    }
    let mut boundaries = Vec::with_capacity(spans.len());
    let mut offset = 0;
    for span in spans {
        offset += span.content.len();
        boundaries.push((offset, span.style));
    }
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut used = 0;
    let mut clipped = false;
    let mut position = from;
    let mut resume = from != FitPosition::default();
    for (offset, grapheme) in text
        .grapheme_indices(true)
        .filter(|(offset, _)| *offset >= from.byte)
    {
        let index = boundaries.partition_point(|(end, _)| *end <= offset);
        let style = boundaries
            .get(index)
            .map_or(Style::default(), |(_, style)| *style);
        let next = FitPosition {
            byte: offset + grapheme.len(),
            tab_remaining: 0,
        };
        if resume {
            used = push_continuation(&mut current, options, style, grapheme.width());
            resume = false;
        }
        if matches!(grapheme, "\n" | "\r\n" | "\r") {
            lines.push((Line::from(std::mem::take(&mut current)), next));
            used = 0;
            clipped = false;
            position = next;
            continue;
        }
        if clipped {
            position = next;
            continue;
        }
        let expanded;
        let is_tab = grapheme == "\t";
        let grapheme = if is_tab {
            let remaining = if offset == from.byte && from.tab_remaining > 0 {
                from.tab_remaining
            } else {
                options.tab_width.max(1) - used % options.tab_width.max(1)
            };
            expanded = " ".repeat(remaining);
            expanded.as_str()
        } else {
            grapheme
        };
        let expanded_len = grapheme.len();
        for (part, grapheme) in grapheme.grapheme_indices(true) {
            let width = grapheme.width();
            let at = FitPosition {
                byte: offset,
                tab_remaining: if is_tab { expanded_len - part } else { 0 },
            };
            if used + width > options.width {
                if !options.wrap {
                    clipped = true;
                    break;
                }
                if used > 0 {
                    lines.push((Line::from(std::mem::take(&mut current)), at));
                    used = push_continuation(&mut current, options, style, width);
                }
                if width > options.width {
                    continue;
                }
            }
            push_span(&mut current, grapheme, style);
            used += width;
        }
        position = next;
    }
    lines.push((Line::from(current), position));
    lines
}

fn push_continuation(
    spans: &mut Vec<Span<'static>>,
    options: FitOptions<'_>,
    style: Style,
    next_width: usize,
) -> usize {
    let mut used = 0;
    for prefix in options.continuation.graphemes(true) {
        let width = prefix.width();
        if used + width + next_width > options.width {
            break;
        }
        push_span(spans, prefix, style);
        used += width;
    }
    used
}

fn push_span(spans: &mut Vec<Span<'static>>, text: &str, style: Style) {
    if let Some(last) = spans.last_mut().filter(|span| span.style == style) {
        last.content.to_mut().push_str(text);
    } else {
        spans.push(Span::styled(text.to_owned(), style));
    }
}
