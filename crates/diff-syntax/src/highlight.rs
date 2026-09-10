//! Tree-sitter syntax highlighting with UTF-8 byte spans and a bounded cache.

use crate::{
    SyntaxError,
    incremental::{AppendContext, Grammars, IncrementalDocument, SyntaxWorkStats},
    language::{LanguageHint, resolve_language},
    spans::{Span, spans_to_flat_tokens},
};
use arborium_theme::tag_to_name;
use clankerdiff_theme::{Fingerprint, HighlightSpan, SyntaxTheme};
use imbl::Vector;
use lru::LruCache;
use std::{
    fmt, mem,
    num::NonZeroUsize,
    ops::Range,
    sync::{Arc, OnceLock},
};

const DEFAULT_MAX_DOCUMENTS: usize = 512;
const DEFAULT_SOURCE_BYTES: usize = 8 * 1024 * 1024;
const MAX_INJECTION_DEPTH: usize = 3;
const DOCUMENT_KEY_DOMAIN: &[u8] = b"syntax-document-v2";

#[must_use]
pub fn empty_spans() -> Arc<[HighlightSpan]> {
    static EMPTY: OnceLock<Arc<[HighlightSpan]>> = OnceLock::new();
    Arc::clone(EMPTY.get_or_init(|| Arc::from(Vec::<HighlightSpan>::new())))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HighlightStats {
    pub calls: u64,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CacheKey {
    fingerprint: Fingerprint,
}

impl CacheKey {
    #[must_use]
    pub const fn fingerprint(self) -> Fingerprint {
        self.fingerprint
    }

    fn document(language: &str, source_id: Fingerprint) -> Self {
        Self {
            fingerprint: Fingerprint::of([
                DOCUMENT_KEY_DOMAIN,
                language.as_bytes(),
                source_id.as_bytes().as_slice(),
            ]),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheConfig {
    pub max_documents: usize,
    pub max_source_bytes: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_documents: DEFAULT_MAX_DOCUMENTS,
            max_source_bytes: DEFAULT_SOURCE_BYTES,
        }
    }
}

impl From<usize> for CacheConfig {
    fn from(max_documents: usize) -> Self {
        Self {
            max_documents,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheUsage {
    pub document_entries: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SyntaxStream {
    hint: String,
    source: String,
    revision: u64,
    theme_revision: Option<Fingerprint>,
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
        Self {
            hint: hint.into().as_str().to_owned(),
            ..Self::default()
        }
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

    fn parse(
        &mut self,
        grammars: &mut Grammars,
        previous_len: usize,
    ) -> Result<usize, SyntaxError> {
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
            slot => slot.insert(grammars.document(language)?.ok_or_else(|| {
                SyntaxError::MissingGrammar {
                    language: language.to_owned(),
                }
            })?),
        };
        document.append(
            &self.source,
            0,
            MAX_INJECTION_DEPTH,
            &mut AppendContext {
                grammars,
                stats: &mut self.work,
                line_starts: &self.line_starts,
            },
        )
    }

    fn update(
        &mut self,
        grammars: &mut Grammars,
        stats: &mut HighlightStats,
        theme: &SyntaxTheme,
        appended: &str,
    ) -> Result<SyntaxStreamUpdate, SyntaxError> {
        let base_revision = self.revision;
        let theme_revision = theme.revision();
        if appended.is_empty() && self.theme_revision == Some(theme_revision) {
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
        if self.line_starts.is_empty() && !self.source.is_empty() {
            self.line_starts.push(0);
        }
        self.line_starts.extend(
            appended
                .bytes()
                .enumerate()
                .filter_map(|(index, byte)| (byte == b'\n').then_some(previous_len + index + 1)),
        );
        let previous_work = self.work.parser_input_bytes;
        let result = self.parse(grammars, previous_len);
        stats.bytes += self.work.parser_input_bytes - previous_work;
        let start = match result {
            Ok(start) => start,
            Err(error) => {
                self.source.truncate(previous_len);
                self.line_starts.truncate(previous_lines);
                self.document = None;
                return Err(error);
            }
        };
        let start = if self.theme_revision == Some(theme_revision) {
            start
        } else {
            0
        };
        let first_changed = self.project(theme, start);
        if !appended.is_empty() {
            self.revision = self.revision.wrapping_add(1);
        }
        self.theme_revision = Some(theme_revision);
        Ok(SyntaxStreamUpdate {
            base_revision,
            revision: self.revision,
            changed_lines: first_changed..self.highlights.line_count(),
            highlights: Arc::clone(&self.highlights),
        })
    }

    fn project(&mut self, theme: &SyntaxTheme, start: usize) -> usize {
        let first_line = self
            .line_starts
            .partition_point(|&offset| offset <= start)
            .saturating_sub(1);
        let byte_start = self.line_starts.get(first_line).copied().unwrap_or(0);
        let raw = self
            .document
            .as_ref()
            .map_or_else(Vec::new, |document| document.spans_from(byte_start));
        let spans = map_spans(theme, &self.source, raw);
        let mut lines = self
            .highlights
            .lines
            .take(first_line.min(self.highlights.line_count()));
        let mut first_changed = lines.len();
        let mut first_span = 0;
        self.work.projected_bytes += self.source.len() - byte_start;
        self.work.reused_lines += lines.len();
        for (line, &from) in self.line_starts.iter().enumerate().skip(first_line) {
            if from == self.source.len() {
                break;
            }
            let mut to = self
                .line_starts
                .get(line + 1)
                .copied()
                .unwrap_or(self.source.len());
            if to > from && self.source.as_bytes()[to - 1] == b'\n' {
                to -= 1;
            }
            if to > from && self.source.as_bytes()[to - 1] == b'\r' {
                to -= 1;
            }
            while spans
                .get(first_span)
                .is_some_and(|span| span.range.end <= from)
            {
                first_span += 1;
            }
            let projected: Vec<_> = spans[first_span..]
                .iter()
                .take_while(|span| span.range.start < to)
                .filter_map(|span| {
                    let start = span.range.start.max(from);
                    let end = span.range.end.min(to);
                    (start < end).then_some(HighlightSpan {
                        range: start - from..end - from,
                        foreground: span.foreground,
                        font_style: span.font_style,
                    })
                })
                .collect();
            self.work.projected_lines += 1;
            if let Some(before) = self
                .highlights
                .lines
                .get(line)
                .filter(|before| before.as_ref() == projected)
            {
                lines.push_back(Arc::clone(before));
                self.work.reused_lines += 1;
                if first_changed == line {
                    first_changed += 1;
                }
            } else {
                lines.push_back(if projected.is_empty() {
                    empty_spans()
                } else {
                    projected.into()
                });
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

pub struct SyntaxHighlighter {
    config: CacheConfig,
    documents: Option<LruCache<CacheKey, SyntaxStream>>,
    stats: HighlightStats,
    grammars: Grammars,
}

impl fmt::Debug for SyntaxHighlighter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyntaxHighlighter")
            .field("config", &self.config)
            .field(
                "documents",
                &self.documents.as_ref().map_or(0, LruCache::len),
            )
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new(CacheConfig::default())
    }
}

impl SyntaxHighlighter {
    #[must_use]
    pub fn new(config: impl Into<CacheConfig>) -> Self {
        let config = config.into();
        Self {
            config,
            documents: NonZeroUsize::new(config.max_documents).map(LruCache::new),
            stats: HighlightStats::default(),
            grammars: Grammars::default(),
        }
    }

    #[must_use]
    pub const fn stats(&self) -> HighlightStats {
        self.stats
    }

    pub fn reset_stats(&mut self) {
        self.stats = HighlightStats::default();
    }

    pub fn take_stats(&mut self) -> HighlightStats {
        mem::take(&mut self.stats)
    }

    #[must_use]
    pub const fn config(&self) -> CacheConfig {
        self.config
    }

    #[must_use]
    pub fn with_theme<'a>(&'a mut self, theme: &'a SyntaxTheme) -> ThemedHighlighter<'a> {
        ThemedHighlighter {
            highlighter: self,
            theme,
        }
    }

    #[must_use]
    pub fn cache_usage(&self) -> CacheUsage {
        CacheUsage {
            document_entries: self.documents.as_ref().map_or(0, LruCache::len),
        }
    }

    pub fn clear_cache(&mut self) {
        if let Some(documents) = &mut self.documents {
            documents.clear();
        }
    }
}

pub struct ThemedHighlighter<'a> {
    highlighter: &'a mut SyntaxHighlighter,
    theme: &'a SyntaxTheme,
}

impl ThemedHighlighter<'_> {
    pub fn highlight_document<'a, T: AsRef<str>>(
        &mut self,
        source_id: Fingerprint,
        language: impl Into<LanguageHint<'a>>,
        source: impl FnOnce() -> T,
    ) -> Result<Arc<DocumentHighlights>, SyntaxError> {
        let language = language.into();
        let key = CacheKey::document(resolve_language(language, "").unwrap_or("auto"), source_id);
        let SyntaxHighlighter {
            config,
            documents,
            stats,
            grammars,
        } = &mut *self.highlighter;
        if let Some(stream) = documents
            .as_mut()
            .and_then(|documents| documents.get_mut(&key))
        {
            stats.calls += 1;
            stats.hits += 1;
            return stream
                .update(grammars, stats, self.theme, "")
                .map(|update| update.highlights);
        }
        let text = source();
        check_limit(config, text.as_ref().len())?;
        stats.calls += 1;
        stats.misses += 1;
        let mut stream = SyntaxStream::new(language);
        let highlights = stream
            .update(grammars, stats, self.theme, text.as_ref())?
            .highlights;
        if let Some(documents) = documents
            && documents.push(key, stream).is_some()
        {
            stats.evictions += 1;
        }
        Ok(highlights)
    }

    pub fn append(
        &mut self,
        stream: &mut SyntaxStream,
        appended: &str,
    ) -> Result<SyntaxStreamUpdate, SyntaxError> {
        let SyntaxHighlighter {
            config,
            stats,
            grammars,
            ..
        } = &mut *self.highlighter;
        check_limit(config, stream.source.len().saturating_add(appended.len()))?;
        stats.calls += 1;
        stream.update(grammars, stats, self.theme, appended)
    }
}

fn check_limit(config: &CacheConfig, attempted: usize) -> Result<(), SyntaxError> {
    let limit = config.max_source_bytes.min(u32::MAX as usize);
    if attempted > limit {
        return Err(SyntaxError::InputLimit { limit, attempted });
    }
    Ok(())
}

fn map_spans(theme: &SyntaxTheme, source: &str, raw_spans: Vec<Span>) -> Vec<HighlightSpan> {
    if raw_spans.is_empty() {
        return Vec::new();
    }
    let tokens = spans_to_flat_tokens(source, raw_spans);
    let mut spans: Vec<HighlightSpan> = Vec::with_capacity(tokens.len());
    for token in tokens {
        let start = token.start as usize;
        let end = token.end as usize;
        if start >= end
            || end > source.len()
            || !source.is_char_boundary(start)
            || !source.is_char_boundary(end)
        {
            continue;
        }
        let Some(capture) = diff_capture_name(token.tag) else {
            continue;
        };
        let Some(style) = theme.style(capture) else {
            continue;
        };
        let span = HighlightSpan {
            range: start..end,
            foreground: style.foreground,
            font_style: style.font_style,
        };
        if let Some(last) = spans.last_mut()
            && last.range.end == span.range.start
            && last.foreground == span.foreground
            && last.font_style == span.font_style
        {
            last.range.end = span.range.end;
        } else {
            spans.push(span);
        }
    }
    spans
}

fn diff_capture_name(tag: &str) -> Option<&'static str> {
    Some(match tag_to_name(tag)? {
        "title" => "markup.heading",
        "strong" => "markup.bold",
        "emphasis" => "markup.italic",
        "link" => "markup.link",
        "literal" => "markup.raw",
        "strikethrough" => "markup.strikethrough",
        name => name,
    })
}
