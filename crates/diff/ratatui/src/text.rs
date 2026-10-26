use ratatui::{
    style::Style,
    text::{Line, Span},
};
use std::iter::from_fn;
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

pub(crate) struct FitCursor<'a> {
    options: FitOptions<'a>,
    position: FitPosition,
    resume: bool,
    prefix_width: Option<usize>,
    exhausted: bool,
}

impl<'a> FitCursor<'a> {
    pub(crate) fn new(mut options: FitOptions<'a>, from: FitPosition) -> Self {
        options.tab_width = options.tab_width.max(1);
        Self {
            options,
            position: from,
            resume: from != FitPosition::default(),
            prefix_width: None,
            exhausted: false,
        }
    }

    pub(crate) const fn position(&self) -> FitPosition {
        self.position
    }

    pub(crate) fn next_row(
        &mut self,
        text: &str,
        mut emit: impl FnMut(usize, &str),
    ) -> Option<FitPosition> {
        if self.exhausted {
            return None;
        }
        debug_assert!(text.is_char_boundary(self.position.byte));
        if self.options.width == 0 {
            self.position = FitPosition {
                byte: text.len(),
                tab_remaining: 0,
            };
            self.exhausted = true;
            return Some(self.position);
        }
        let mut used = 0;
        let mut clipped = false;
        let start = self.position.byte;
        for (relative, grapheme) in text[start..].grapheme_indices(true) {
            let offset = start + relative;
            let next = FitPosition {
                byte: offset + grapheme.len(),
                tab_remaining: 0,
            };
            if self.resume {
                let next_width = self.prefix_width.take().unwrap_or_else(|| grapheme.width());
                used = emit_continuation(self.options, next_width, |piece| emit(offset, piece));
                self.resume = false;
            }
            if matches!(grapheme, "\n" | "\r\n" | "\r") {
                self.position = next;
                return Some(next);
            }
            if clipped {
                self.position = next;
                continue;
            }
            let is_tab = grapheme == "\t";
            let mut remaining = if is_tab {
                if self.position.tab_remaining > 0 {
                    self.position.tab_remaining
                } else {
                    self.options.tab_width - used % self.options.tab_width
                }
            } else {
                1
            };
            let piece = if is_tab { " " } else { grapheme };
            let width = piece.width();
            while remaining > 0 {
                if used + width > self.options.width {
                    if !self.options.wrap {
                        clipped = true;
                        break;
                    }
                    if used > 0 {
                        self.position = FitPosition {
                            byte: offset,
                            tab_remaining: if is_tab { remaining } else { 0 },
                        };
                        self.resume = true;
                        self.prefix_width = Some(width);
                        return Some(self.position);
                    }
                    if width > self.options.width {
                        break;
                    }
                }
                emit(offset, piece);
                used += width;
                remaining -= 1;
            }
            self.position = next;
        }
        self.exhausted = true;
        Some(self.position)
    }
}

pub(crate) fn fit_spans_from<'a>(
    spans: Vec<Span<'_>>,
    options: FitOptions<'a>,
    from: FitPosition,
) -> impl Iterator<Item = (Line<'static>, FitPosition)> + 'a {
    let text: String = spans.iter().map(|span| span.content.as_ref()).collect();
    let mut boundaries = Vec::with_capacity(spans.len());
    let mut offset = 0;
    for span in spans {
        offset += span.content.len();
        boundaries.push((offset, span.style));
    }
    let mut cursor = FitCursor::new(options, from);
    from_fn(move || {
        let mut spans = Vec::new();
        let end = cursor.next_row(&text, |offset, piece| {
            let index = boundaries.partition_point(|(end, _)| *end <= offset);
            let style = boundaries
                .get(index)
                .map_or(Style::default(), |(_, style)| *style);
            push_span(&mut spans, piece, style);
        })?;
        Some((Line::from(spans), end))
    })
}

fn emit_continuation(
    options: FitOptions<'_>,
    next_width: usize,
    mut emit: impl FnMut(&str),
) -> usize {
    let mut used = 0;
    for prefix in options.continuation.graphemes(true) {
        let width = prefix.width();
        if used + width + next_width > options.width {
            break;
        }
        emit(prefix);
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
