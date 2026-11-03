use clankerdiff_ui_core::{HighlightId, HighlightSpan};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub(crate) struct Span {
    pub start: u32,
    pub end: u32,
    pub highlight: Option<HighlightId>,
    pub pattern_index: u32,
}

pub(crate) struct Injection {
    pub start: u32,
    pub end: u32,
    pub language: String,
}

pub(crate) fn flatten(source: &str, spans: Vec<Span>) -> Vec<HighlightSpan> {
    let spans = normalize(spans);
    let mut events = Vec::with_capacity(spans.len() * 2);
    for (index, span) in spans.iter().enumerate() {
        events.push((span.range.start, true, index));
        events.push((span.range.end, false, index));
    }
    events.sort_by_key(|&(position, start, _)| (position, start));
    let length = source.trim_end_matches('\n').len();
    let mut flat: Vec<HighlightSpan> = Vec::new();
    let mut active: Vec<usize> = Vec::new();
    let mut previous = 0;
    for (position, start, index) in events {
        if position > previous && position <= length {
            if let Some(&top) = active.last() {
                let highlight = spans[top].highlight;
                if let Some(last) = flat.last_mut()
                    && last.highlight == highlight
                    && last.range.end == previous
                {
                    last.range.end = position;
                } else {
                    flat.push(HighlightSpan { range: previous..position, highlight });
                }
            }
            previous = position;
        }
        if start {
            active.push(index);
        } else if let Some(offset) = active.iter().rposition(|&entry| entry == index) {
            active.remove(offset);
        }
    }
    flat.retain(|span| source.is_char_boundary(span.range.start) && source.is_char_boundary(span.range.end));
    flat
}

fn normalize(spans: Vec<Span>) -> Vec<HighlightSpan> {
    let mut unique: BTreeMap<(u32, u32), Span> = BTreeMap::new();
    for span in spans {
        let key = (span.start, span.end);
        let priority = |span: &Span| (span.highlight.is_some(), span.pattern_index);
        if unique.get(&key).is_none_or(|existing| priority(&span) >= priority(existing)) {
            unique.insert(key, span);
        }
    }
    let mut normalized: Vec<HighlightSpan> = Vec::new();
    for span in unique.into_values() {
        let Some(highlight) = span.highlight else {
            continue;
        };
        let range = span.start as usize..span.end as usize;
        if let Some(last) = normalized.last_mut()
            && last.highlight == highlight
            && range.start <= last.range.end
        {
            last.range.end = last.range.end.max(range.end);
        } else {
            normalized.push(HighlightSpan { range, highlight });
        }
    }
    normalized.sort_by(|a, b| a.range.start.cmp(&b.range.start).then_with(|| b.range.end.cmp(&a.range.end)));
    normalized
}
