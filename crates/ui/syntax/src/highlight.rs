use crate::{
    SyntaxError,
    incremental::{AppendContext, IncrementalDocument, SyntaxWorkStats},
    language::{LanguageHint, resolve_language},
    spans::flatten,
    symbols::SymbolTags,
};
use clankerdiff_ui_core::{HighlightId, HighlightSpan};
use std::{cell::Cell, fmt, iter, ops::Range, slice, sync::Arc};

pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
const MAX_INJECTION_DEPTH: usize = 3;
const CHUNK_LINES: usize = 256;

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
        let mut raw = Vec::new();
        if let Some(document) = &self.document {
            document.spans_from(byte_start, &mut raw);
        }
        let spans = flatten(&self.source, raw);
        let previous = &self.highlights;
        let mut lines = HighlightsBuilder::prefix(previous, first_line.min(previous.line_count()));
        let mut first_changed = lines.line_count();
        let mut first_span = 0;
        work.projected_bytes += self.source.len() - byte_start;
        work.reused_lines += lines.line_count();
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
            let projected = spans[first_span..].iter().take_while(|span| span.range.start < to).filter_map(|span| {
                let start = span.range.start.max(from);
                let end = span.range.end.min(to);
                (start < end).then(|| (relative(start - from), relative(end - from), span.highlight))
            });
            work.projected_lines += 1;
            if lines.push_line(projected, previous.line(line)) {
                work.reused_lines += 1;
                if first_changed == line {
                    first_changed += 1;
                }
            }
        }
        self.highlights = Arc::new(lines.finish());
        first_changed
    }
}

fn relative(offset: usize) -> u32 {
    u32::try_from(offset).expect("source limit fits u32")
}

#[derive(Debug, Clone, Default)]
pub struct DocumentHighlights {
    chunks: Vec<Arc<HighlightChunk>>,
    lines: usize,
}

impl DocumentHighlights {
    #[must_use]
    pub fn line(&self, index: usize) -> Option<HighlightLine<'_>> {
        (index < self.lines).then(|| self.chunks[index / CHUNK_LINES].line(index % CHUNK_LINES))
    }

    #[must_use]
    pub const fn line_count(&self) -> usize {
        self.lines
    }

    pub fn lines(&self) -> impl ExactSizeIterator<Item = HighlightLine<'_>> {
        (0..self.lines).map(|index| self.chunks[index / CHUNK_LINES].line(index % CHUNK_LINES))
    }
}

#[derive(Debug, Clone, Default)]
struct HighlightChunk {
    line_ends: Vec<u32>,
    starts: Vec<u32>,
    ends: Vec<u32>,
    ids: Vec<HighlightId>,
}

impl HighlightChunk {
    fn line(&self, index: usize) -> HighlightLine<'_> {
        let from = index.checked_sub(1).map_or(0, |previous| self.line_ends[previous] as usize);
        let to = self.line_ends[index] as usize;
        HighlightLine { starts: &self.starts[from..to], ends: &self.ends[from..to], ids: &self.ids[from..to] }
    }

    fn prefix(&self, lines: usize) -> Self {
        let spans = lines.checked_sub(1).map_or(0, |last| self.line_ends[last] as usize);
        Self {
            line_ends: self.line_ends[..lines].to_vec(),
            starts: self.starts[..spans].to_vec(),
            ends: self.ends[..spans].to_vec(),
            ids: self.ids[..spans].to_vec(),
        }
    }
}

struct HighlightsBuilder {
    chunks: Vec<Arc<HighlightChunk>>,
    tail: HighlightChunk,
    lines: usize,
}

impl HighlightsBuilder {
    fn prefix(previous: &DocumentHighlights, lines: usize) -> Self {
        let full = lines / CHUNK_LINES;
        let partial = lines % CHUNK_LINES;
        let tail = if partial == 0 { HighlightChunk::default() } else { previous.chunks[full].prefix(partial) };
        Self { chunks: previous.chunks[..full].to_vec(), tail, lines }
    }

    const fn line_count(&self) -> usize {
        self.lines
    }

    fn push_line(
        &mut self,
        spans: impl Iterator<Item = (u32, u32, HighlightId)>,
        previous: Option<HighlightLine<'_>>,
    ) -> bool {
        let tail = &mut self.tail;
        for (start, end, id) in spans {
            tail.starts.push(start);
            tail.ends.push(end);
            tail.ids.push(id);
        }
        tail.line_ends.push(u32::try_from(tail.ids.len()).expect("chunk spans fit u32"));
        let unchanged = previous == Some(tail.line(tail.line_ends.len() - 1));
        self.lines += 1;
        if tail.line_ends.len() == CHUNK_LINES {
            self.chunks.push(Arc::new(std::mem::take(tail)));
        }
        unchanged
    }

    fn finish(mut self) -> DocumentHighlights {
        if !self.tail.line_ends.is_empty() {
            self.chunks.push(Arc::new(self.tail));
        }
        DocumentHighlights { chunks: self.chunks, lines: self.lines }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct HighlightLine<'a> {
    starts: &'a [u32],
    ends: &'a [u32],
    ids: &'a [HighlightId],
}

type SpanParts<'a> = iter::Zip<iter::Zip<slice::Iter<'a, u32>, slice::Iter<'a, u32>>, slice::Iter<'a, HighlightId>>;

pub type HighlightSpans<'a> = iter::Map<SpanParts<'a>, fn(((&u32, &u32), &HighlightId)) -> HighlightSpan>;

impl<'a> HighlightLine<'a> {
    #[must_use]
    pub const fn len(&self) -> usize {
        self.ids.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn iter(&self) -> HighlightSpans<'a> {
        self.starts
            .iter()
            .zip(self.ends)
            .zip(self.ids)
            .map(|((&start, &end), &highlight)| HighlightSpan { range: start as usize..end as usize, highlight })
    }
}

impl<'a> IntoIterator for HighlightLine<'a> {
    type Item = HighlightSpan;
    type IntoIter = HighlightSpans<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &HighlightLine<'a> {
    type Item = HighlightSpan;
    type IntoIter = HighlightSpans<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl fmt::Debug for HighlightLine<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
