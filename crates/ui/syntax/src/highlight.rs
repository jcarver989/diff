use crate::{
    SyntaxError,
    incremental::{AppendContext, IncrementalDocument, SyntaxWorkStats},
    language::{LanguageHint, resolve_language},
    spans::{Span, spans_to_flat_tokens},
    symbols::SymbolTags,
};
use clankerdiff_ui_core::HighlightSpan;
use imbl::Vector;
use std::{
    cell::Cell,
    ops::Range,
    sync::{Arc, OnceLock},
};

pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
const MAX_INJECTION_DEPTH: usize = 3;

#[must_use]
pub fn empty_spans() -> Arc<[HighlightSpan]> {
    static EMPTY: OnceLock<Arc<[HighlightSpan]>> = OnceLock::new();
    Arc::clone(EMPTY.get_or_init(|| Arc::from(Vec::<HighlightSpan>::new())))
}

thread_local! {
    static THREAD_WORK: Cell<SyntaxWorkStats> = Cell::default();
}

#[must_use]
pub fn thread_work() -> SyntaxWorkStats {
    THREAD_WORK.get()
}

#[derive(Debug, Clone, Default)]
pub struct SourceSyntax {
    pub highlights: Arc<DocumentHighlights>,
    pub symbols: Arc<SymbolTags>,
}

#[must_use]
pub fn analyze<'a>(language: impl Into<LanguageHint<'a>>, source: &str) -> SourceSyntax {
    let mut stream = SyntaxStream::new(language);
    let _rejected = stream.append(source);
    stream.syntax()
}

#[derive(Debug, Clone, Default)]
pub struct SyntaxStream {
    hint: String,
    source: String,
    revision: u64,
    highlights: Arc<DocumentHighlights>,
    document: Option<IncrementalDocument>,
    language: Option<&'static str>,
    line_starts: Vec<usize>,
    work: SyntaxWorkStats,
}

#[derive(Debug, Clone)]
pub struct SyntaxStreamUpdate {
    pub base_revision: u64,
    pub revision: u64,
    pub changed_lines: Range<usize>,
    pub highlights: Arc<DocumentHighlights>,
}

impl SyntaxStream {
    #[must_use]
    pub fn new<'a>(hint: impl Into<LanguageHint<'a>>) -> Self {
        Self { hint: hint.into().as_str().to_owned(), ..Self::default() }
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn highlights(&self) -> &Arc<DocumentHighlights> {
        &self.highlights
    }

    #[must_use]
    pub const fn work_stats(&self) -> SyntaxWorkStats {
        self.work
    }

    #[must_use]
    pub fn syntax(&self) -> SourceSyntax {
        let symbols = self
            .document
            .as_ref()
            .and_then(|document| Some(document.grammar().tags()?.extract(document.tree()?, &self.source)))
            .unwrap_or_default();
        SourceSyntax { highlights: Arc::clone(&self.highlights), symbols: Arc::new(symbols) }
    }

    pub fn append(&mut self, appended: &str) -> Result<SyntaxStreamUpdate, SyntaxError> {
        let base_revision = self.revision;
        let attempted = self.source.len().saturating_add(appended.len());
        if attempted > MAX_SOURCE_BYTES {
            return Err(SyntaxError::InputLimit { limit: MAX_SOURCE_BYTES, attempted });
        }
        if appended.is_empty() {
            let end = self.highlights.line_count();
            return Ok(SyntaxStreamUpdate {
                base_revision,
                revision: self.revision,
                changed_lines: end..end,
                highlights: Arc::clone(&self.highlights),
            });
        }
        let previous_len = self.source.len();
        let previous_lines = self.line_starts.len();
        self.source.push_str(appended);
        if self.line_starts.is_empty() {
            self.line_starts.push(0);
        }
        self.line_starts.extend(
            appended
                .bytes()
                .enumerate()
                .filter_map(|(index, byte)| (byte == b'\n').then_some(previous_len + index + 1)),
        );
        let mut work = SyntaxWorkStats::default();
        let parsed = self.parse(previous_len, &mut work).map(|start| self.project(start, &mut work));
        self.work = self.work + work;
        THREAD_WORK.set(THREAD_WORK.get() + work);
        let first_changed = match parsed {
            Ok(first_changed) => first_changed,
            Err(error) => {
                self.source.truncate(previous_len);
                self.line_starts.truncate(previous_lines);
                self.document = None;
                return Err(error);
            }
        };
        self.revision = self.revision.wrapping_add(1);
        Ok(SyntaxStreamUpdate {
            base_revision,
            revision: self.revision,
            changed_lines: first_changed..self.highlights.line_count(),
            highlights: Arc::clone(&self.highlights),
        })
    }

    fn parse(&mut self, previous_len: usize, work: &mut SyntaxWorkStats) -> Result<usize, SyntaxError> {
        let resolved = resolve_language(self.hint.as_str(), &self.source);
        let language_changed = resolved != self.language;
        if language_changed {
            self.document = None;
            self.language = resolved;
        }
        let Some(language) = resolved else {
            return Ok(if language_changed { 0 } else { previous_len });
        };
        let document = match &mut self.document {
            Some(document) => document,
            slot => slot.insert(
                IncrementalDocument::new(language)
                    .ok_or_else(|| SyntaxError::MissingGrammar { language: language.to_owned() })?,
            ),
        };
        document.append(
            &self.source,
            0,
            MAX_INJECTION_DEPTH,
            &mut AppendContext { stats: work, line_starts: &self.line_starts },
        )
    }

    fn project(&mut self, start: usize, work: &mut SyntaxWorkStats) -> usize {
        let first_line = self.line_starts.partition_point(|&offset| offset <= start).saturating_sub(1);
        let byte_start = self.line_starts.get(first_line).copied().unwrap_or(0);
        let raw = self.document.as_ref().map_or_else(Vec::new, |document| document.spans_from(byte_start));
        let spans = map_spans(&self.source, raw);
        let mut lines = self.highlights.lines.take(first_line.min(self.highlights.line_count()));
        let mut first_changed = lines.len();
        let mut first_span = 0;
        work.projected_bytes += self.source.len() - byte_start;
        work.reused_lines += lines.len();
        for (line, &from) in self.line_starts.iter().enumerate().skip(first_line) {
            if from == self.source.len() {
                break;
            }
            let mut to = self.line_starts.get(line + 1).copied().unwrap_or(self.source.len());
            if to > from && self.source.as_bytes()[to - 1] == b'\n' {
                to -= 1;
            }
            if to > from && self.source.as_bytes()[to - 1] == b'\r' {
                to -= 1;
            }
            while spans.get(first_span).is_some_and(|span| span.range.end <= from) {
                first_span += 1;
            }
            let projected: Vec<_> = spans[first_span..]
                .iter()
                .take_while(|span| span.range.start < to)
                .filter_map(|span| {
                    let start = span.range.start.max(from);
                    let end = span.range.end.min(to);
                    (start < end)
                        .then_some(HighlightSpan { range: start - from..end - from, highlight: span.highlight })
                })
                .collect();
            work.projected_lines += 1;
            if let Some(before) = self.highlights.lines.get(line).filter(|before| before.as_ref() == projected) {
                lines.push_back(Arc::clone(before));
                work.reused_lines += 1;
                if first_changed == line {
                    first_changed += 1;
                }
            } else {
                lines.push_back(if projected.is_empty() { empty_spans() } else { projected.into() });
            }
        }
        self.highlights = Arc::new(DocumentHighlights { lines });
        first_changed
    }
}

#[derive(Debug, Clone, Default)]
pub struct DocumentHighlights {
    lines: Vector<Arc<[HighlightSpan]>>,
}

impl DocumentHighlights {
    #[must_use]
    pub fn line(&self, index: usize) -> Option<&[HighlightSpan]> {
        self.lines.get(index).map(AsRef::as_ref)
    }

    #[must_use]
    pub fn line_shared(&self, index: usize) -> Option<Arc<[HighlightSpan]>> {
        self.lines.get(index).cloned()
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
}

fn map_spans(source: &str, raw_spans: Vec<Span>) -> Vec<HighlightSpan> {
    if raw_spans.is_empty() {
        return Vec::new();
    }
    let tokens = spans_to_flat_tokens(source, raw_spans);
    let mut spans: Vec<HighlightSpan> = Vec::with_capacity(tokens.len());
    for token in tokens {
        let start = token.start as usize;
        let end = token.end as usize;
        if start >= end || end > source.len() || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
            continue;
        }
        if let Some(last) = spans.last_mut()
            && last.range.end == start
            && last.highlight == token.highlight
        {
            last.range.end = end;
        } else {
            spans.push(HighlightSpan { range: start..end, highlight: token.highlight });
        }
    }
    spans
}
