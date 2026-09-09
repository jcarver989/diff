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

pub(crate) fn fit_spans(spans: Vec<Span<'static>>, options: FitOptions<'_>) -> Vec<Line<'static>> {
    if options.width == 0 {
        return vec![Line::default()];
    }
    let text: String = spans.iter().map(|span| span.content.as_ref()).collect();
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
    for (offset, grapheme) in text.grapheme_indices(true) {
        let index = boundaries.partition_point(|(end, _)| *end <= offset);
        let style = boundaries
            .get(index)
            .map_or(Style::default(), |(_, style)| *style);
        if matches!(grapheme, "\n" | "\r\n" | "\r") {
            lines.push(Line::from(std::mem::take(&mut current)));
            used = 0;
            clipped = false;
            continue;
        }
        if clipped {
            continue;
        }
        let expanded;
        let grapheme = if grapheme == "\t" {
            expanded = " ".repeat(options.tab_width.max(1) - used % options.tab_width.max(1));
            expanded.as_str()
        } else {
            grapheme
        };
        for grapheme in grapheme.graphemes(true) {
            let width = grapheme.width();
            if used + width > options.width {
                if !options.wrap {
                    clipped = true;
                    break;
                }
                if used > 0 {
                    lines.push(Line::from(std::mem::take(&mut current)));
                    used = 0;
                    for prefix in options.continuation.graphemes(true) {
                        let prefix_width = prefix.width();
                        if used + prefix_width + width > options.width {
                            break;
                        }
                        push_span(&mut current, prefix, style);
                        used += prefix_width;
                    }
                }
                if width > options.width {
                    continue;
                }
            }
            push_span(&mut current, grapheme, style);
            used += width;
        }
    }
    lines.push(Line::from(current));
    lines
}

fn push_span(spans: &mut Vec<Span<'static>>, text: &str, style: Style) {
    if let Some(last) = spans.last_mut().filter(|span| span.style == style) {
        last.content.to_mut().push_str(text);
    } else {
        spans.push(Span::styled(text.to_owned(), style));
    }
}
