use arborium_theme::tag_for_capture;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub(crate) struct Span {
    pub start: u32,
    pub end: u32,
    pub capture: String,
    pub pattern_index: u32,
}

pub(crate) struct Injection {
    pub start: u32,
    pub end: u32,
    pub language: String,
}

pub(crate) struct FlatToken {
    pub start: u32,
    pub end: u32,
    pub tag: &'static str,
}

pub(crate) fn spans_to_flat_tokens(source: &str, spans: Vec<Span>) -> Vec<FlatToken> {
    let spans = normalize(spans);
    let mut events = Vec::with_capacity(spans.len() * 2);
    for (index, span) in spans.iter().enumerate() {
        events.push((span.start, true, index));
        events.push((span.end, false, index));
    }
    events.sort_by_key(|&(position, start, _)| (position, start));
    let length = source.trim_end_matches('\n').len();
    let mut tokens: Vec<FlatToken> = Vec::new();
    let mut active: Vec<usize> = Vec::new();
    let mut previous = 0;
    for (position, start, index) in events {
        if position > previous && position as usize <= length {
            if let Some(&top) = active.last() {
                let tag = spans[top].tag;
                if let Some(last) = tokens.last_mut()
                    && last.tag == tag
                    && last.end == previous
                {
                    last.end = position;
                } else {
                    tokens.push(FlatToken {
                        start: previous,
                        end: position,
                        tag,
                    });
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
    tokens
}

fn normalize(spans: Vec<Span>) -> Vec<FlatToken> {
    let mut unique: BTreeMap<(u32, u32), Span> = BTreeMap::new();
    for span in spans {
        let key = (span.start, span.end);
        let priority = |span: &Span| (tag_for_capture(&span.capture).is_some(), span.pattern_index);
        if unique
            .get(&key)
            .is_none_or(|existing| priority(&span) >= priority(existing))
        {
            unique.insert(key, span);
        }
    }
    let mut normalized: Vec<FlatToken> = Vec::new();
    for span in unique.into_values() {
        let Some(tag) = tag_for_capture(&span.capture) else {
            continue;
        };
        if let Some(last) = normalized.last_mut()
            && last.tag == tag
            && span.start <= last.end
        {
            last.end = last.end.max(span.end);
        } else {
            normalized.push(FlatToken {
                start: span.start,
                end: span.end,
                tag,
            });
        }
    }
    normalized.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| b.end.cmp(&a.end)));
    normalized
}
