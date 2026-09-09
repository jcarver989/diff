//! Tree-sitter syntax highlighting with UTF-8 byte spans and a bounded cache.

use crate::language::{LanguageHint, resolve_language};
use arborium::{Config, Highlighter};
use arborium_highlight::spans_to_flat_tokens;
use arborium_theme::tag_to_name;
use clankerdiff_fingerprint::SourceSequenceId;
use clankerdiff_theme::{Fingerprint, HighlightSpan, SyntaxTheme};
use lru::LruCache;
use std::{
    fmt,
    num::NonZeroUsize,
    ops::Range,
    sync::{Arc, OnceLock},
};
const DEFAULT_CAPACITY: usize = 512;
const DEFAULT_MAX_DOCUMENTS: usize = 32;
const DEFAULT_STREAM_BYTES: usize = 8 * 1024 * 1024;
const SOURCE_KEY_DOMAIN: &[u8] = b"syntax-source-v1";
const DOCUMENT_KEY_DOMAIN: &[u8] = b"syntax-document-v1";

/// A shared empty span set, for text with nothing to highlight.
#[must_use]
pub fn empty_spans() -> Arc<[HighlightSpan]> {
    static EMPTY: OnceLock<Arc<[HighlightSpan]>> = OnceLock::new();
    Arc::clone(EMPTY.get_or_init(|| Arc::from(Vec::<HighlightSpan>::new())))
}

/// Counters useful for measuring highlighting and cache behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HighlightStats {
    pub calls: u64,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    /// Bytes actually supplied to Tree-sitter parsers.
    pub bytes: usize,
}

/// Opaque syntax-cache key with a stable diagnostic fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CacheKey {
    fingerprint: Fingerprint,
}

impl CacheKey {
    #[must_use]
    pub const fn fingerprint(self) -> Fingerprint {
        self.fingerprint
    }

    fn source(theme: Fingerprint, language: &str, source: &str) -> Self {
        Self::new([
            SOURCE_KEY_DOMAIN,
            theme.as_bytes().as_slice(),
            language.as_bytes(),
            source.as_bytes(),
        ])
    }

    fn document(theme: Fingerprint, language: &str, sequence: SourceSequenceId) -> Self {
        let sequence = Fingerprint::from(sequence);
        Self::new([
            DOCUMENT_KEY_DOMAIN,
            theme.as_bytes().as_slice(),
            language.as_bytes(),
            sequence.as_bytes().as_slice(),
        ])
    }

    fn new<const N: usize>(fields: [&[u8]; N]) -> Self {
        Self {
            fingerprint: Fingerprint::of(fields),
        }
    }
}

/// Fixed resource limits for a [`SyntaxHighlighter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheConfig {
    pub max_entries: usize,
    pub max_documents: usize,
    pub max_stream_bytes: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: DEFAULT_CAPACITY,
            max_documents: DEFAULT_MAX_DOCUMENTS,
            max_stream_bytes: DEFAULT_STREAM_BYTES,
        }
    }
}

impl From<usize> for CacheConfig {
    fn from(max_entries: usize) -> Self {
        Self {
            max_entries,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheUsage {
    pub span_entries: usize,
    pub document_entries: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SyntaxStream {
    hint: String,
    source: String,
    revision: u64,
    theme_revision: Option<Fingerprint>,
    highlights: Arc<DocumentHighlights>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SyntaxStreamError {
    #[error("syntax stream input requires {attempted} bytes, exceeding the {limit}-byte limit")]
    InputLimit { limit: usize, attempted: usize },
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
}

/// Highlight spans projected onto every source line of one parsed document.
#[derive(Debug, Clone, Default)]
pub struct DocumentHighlights {
    lines: Vec<Arc<[HighlightSpan]>>,
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

    fn from_spans(spans: &[HighlightSpan], text: &str) -> Self {
        let starts = if text.is_empty() {
            Vec::new()
        } else {
            let mut starts = vec![0];
            starts.extend(
                text.bytes()
                    .enumerate()
                    .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
            );
            if text.ends_with('\n') {
                starts.pop();
            }
            starts
        };
        let mut lines = Vec::with_capacity(starts.len());
        let mut first_span = 0;
        for (line, &start) in starts.iter().enumerate() {
            let next = starts.get(line + 1).copied().unwrap_or(text.len());
            let mut end = next;
            if end > start && text.as_bytes()[end - 1] == b'\n' {
                end -= 1;
            }
            if end > start && text.as_bytes()[end - 1] == b'\r' {
                end -= 1;
            }
            while spans
                .get(first_span)
                .is_some_and(|span| span.range.end <= start)
            {
                first_span += 1;
            }
            let projected = spans[first_span..]
                .iter()
                .take_while(|span| span.range.start < end)
                .filter_map(|span| {
                    let from = span.range.start.max(start);
                    let to = span.range.end.min(end);
                    (from < to).then_some(HighlightSpan {
                        range: from - start..to - start,
                        foreground: span.foreground,
                        font_style: span.font_style,
                    })
                })
                .collect::<Vec<_>>();
            lines.push(Arc::from(projected));
        }
        Self { lines }
    }
}

/// A reusable syntax highlighter. Least-recently-used entries are evicted when full.
pub struct SyntaxHighlighter {
    highlighter: Highlighter,
    config: CacheConfig,
    cache: Option<LruCache<CacheKey, Arc<[HighlightSpan]>>>,
    documents: Option<LruCache<CacheKey, Arc<DocumentHighlights>>>,
    stats: HighlightStats,
}

impl fmt::Debug for SyntaxHighlighter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyntaxHighlighter")
            .field("config", &self.config)
            .field("entries", &self.cache.as_ref().map_or(0, LruCache::len))
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
        Self::new(DEFAULT_CAPACITY)
    }
}

impl SyntaxHighlighter {
    /// Creates a highlighter with fixed cache resource limits.
    #[must_use]
    pub fn new(config: impl Into<CacheConfig>) -> Self {
        let syntax_config = Config {
            max_injection_depth: 3,
            ..Config::default()
        };
        let config = config.into();
        let document_entries = if config.max_entries == 0 {
            0
        } else {
            config.max_documents
        };
        Self {
            highlighter: Highlighter::with_config(syntax_config),
            config,
            cache: NonZeroUsize::new(config.max_entries).map(LruCache::new),
            documents: NonZeroUsize::new(document_entries).map(LruCache::new),
            stats: HighlightStats::default(),
        }
    }

    #[must_use]
    pub const fn stats(&self) -> HighlightStats {
        self.stats
    }

    pub fn reset_stats(&mut self) {
        self.stats = HighlightStats::default();
    }

    /// Atomically returns all counters accumulated so far and resets them.
    pub fn take_stats(&mut self) -> HighlightStats {
        std::mem::take(&mut self.stats)
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
            span_entries: self.cache.as_ref().map_or(0, LruCache::len),
            document_entries: self.documents.as_ref().map_or(0, LruCache::len),
        }
    }

    pub fn clear_cache(&mut self) {
        if let Some(cache) = &mut self.cache {
            cache.clear();
        }
        if let Some(documents) = &mut self.documents {
            documents.clear();
        }
    }

    fn highlight_source(
        &mut self,
        theme: &SyntaxTheme,
        hint: LanguageHint<'_>,
        text: &str,
    ) -> Arc<[HighlightSpan]> {
        self.stats.calls += 1;
        let language = resolve_language(hint, text);
        let id = language.unwrap_or("plain");
        let key = CacheKey::source(theme.revision(), id, text);
        if let Some(spans) = self.cache.as_mut().and_then(|cache| cache.get(&key)) {
            self.stats.hits += 1;
            return Arc::clone(spans);
        }
        self.stats.misses += 1;
        let Some(language) = language else {
            let spans = empty_spans();
            self.store(key, Arc::clone(&spans));
            return spans;
        };
        self.stats.bytes = self.stats.bytes.saturating_add(text.len());
        let spans = highlight_source(&mut self.highlighter, theme, language, text)
            .map_or_else(empty_spans, Arc::from);
        self.store(key, Arc::clone(&spans));
        spans
    }

    fn highlight_lines<'line, T>(
        &mut self,
        theme: &SyntaxTheme,
        hint: LanguageHint<'_>,
        lines: T,
    ) -> Vec<Vec<HighlightSpan>>
    where
        T: IntoIterator<Item = &'line str>,
    {
        self.stats.calls += 1;
        let selected: Vec<(usize, &str)> = lines.into_iter().enumerate().collect();
        let window = JoinedLines::new(selected);
        let Some(language) = resolve_language(hint, &window.source) else {
            return vec![Vec::new(); window.lines.len()];
        };
        self.stats.bytes = self.stats.bytes.saturating_add(window.source.len());
        highlight_source(&mut self.highlighter, theme, language, &window.source)
            .as_deref()
            .map_or_else(
                || vec![Vec::new(); window.lines.len()],
                |spans| window.split(spans),
            )
    }

    fn store(&mut self, key: CacheKey, spans: Arc<[HighlightSpan]>) {
        if let Some(cache) = &mut self.cache
            && let Some((evicted, _)) = cache.push(key, spans)
            && evicted != key
        {
            self.stats.evictions += 1;
        }
    }

    fn store_document(&mut self, key: CacheKey, highlights: Arc<DocumentHighlights>) {
        if let Some(documents) = &mut self.documents
            && let Some((evicted, _)) = documents.push(key, highlights)
            && evicted != key
        {
            self.stats.evictions += 1;
        }
    }
}

/// Theme-bound highlighting operations.
pub struct ThemedHighlighter<'a> {
    highlighter: &'a mut SyntaxHighlighter,
    theme: &'a SyntaxTheme,
}

impl ThemedHighlighter<'_> {
    /// Parses a complete source on first use and caches line-projected spans.
    pub fn highlight_document<'a>(
        &mut self,
        sequence: SourceSequenceId,
        language: impl Into<LanguageHint<'a>>,
        text: &str,
    ) -> Arc<DocumentHighlights> {
        self.highlighter.stats.calls += 1;
        let resolved = resolve_language(language.into(), text);
        let key = CacheKey::document(self.theme.revision(), resolved.unwrap_or("plain"), sequence);
        if let Some(highlights) = self
            .highlighter
            .documents
            .as_mut()
            .and_then(|documents| documents.get(&key))
        {
            self.highlighter.stats.hits += 1;
            return Arc::clone(highlights);
        }
        self.parse_document(key, resolved, text)
    }

    /// Parses a line sequence as one complete document on first use and caches
    /// line-projected spans. The lines are only joined and parsed on a miss.
    pub fn highlight_document_lines<'a, 'line>(
        &mut self,
        sequence: SourceSequenceId,
        language: impl Into<LanguageHint<'a>>,
        lines: impl IntoIterator<Item = &'line str>,
    ) -> Arc<DocumentHighlights> {
        self.highlighter.stats.calls += 1;
        let mut lines = lines.into_iter().peekable();
        let resolved = resolve_language(language.into(), lines.peek().copied().unwrap_or_default());
        let key = CacheKey::document(self.theme.revision(), resolved.unwrap_or("plain"), sequence);
        if let Some(highlights) = self
            .highlighter
            .documents
            .as_mut()
            .and_then(|documents| documents.get(&key))
        {
            self.highlighter.stats.hits += 1;
            return Arc::clone(highlights);
        }
        let mut text = String::new();
        for line in lines {
            text.push_str(line);
            text.push('\n');
        }
        self.parse_document(key, resolved, &text)
    }

    fn parse_document(
        &mut self,
        key: CacheKey,
        resolved: Option<&str>,
        text: &str,
    ) -> Arc<DocumentHighlights> {
        self.highlighter.stats.misses += 1;
        let highlights = Arc::new(self.project(resolved, text));
        self.highlighter
            .store_document(key, Arc::clone(&highlights));
        highlights
    }

    fn project(&mut self, resolved: Option<&str>, text: &str) -> DocumentHighlights {
        let spans = resolved
            .and_then(|language| {
                self.highlighter.stats.bytes += text.len();
                highlight_source(
                    &mut self.highlighter.highlighter,
                    self.theme,
                    language,
                    text,
                )
            })
            .unwrap_or_default();
        DocumentHighlights::from_spans(&spans, text)
    }

    /// Highlights a complete source. Hints may be IDs, aliases, or repository paths.
    pub fn highlight_source<'a>(
        &mut self,
        language: impl Into<LanguageHint<'a>>,
        text: &str,
    ) -> Arc<[HighlightSpan]> {
        self.highlighter
            .highlight_source(self.theme, language.into(), text)
    }

    /// Highlights all supplied lines in one parse, preserving multiline state.
    pub fn highlight_lines<'line, 'hint, T>(
        &mut self,
        language: impl Into<LanguageHint<'hint>>,
        lines: T,
    ) -> Vec<Vec<HighlightSpan>>
    where
        T: IntoIterator<Item = &'line str>,
    {
        self.highlighter
            .highlight_lines(self.theme, language.into(), lines)
    }

    pub fn append<'line>(
        &mut self,
        stream: &mut SyntaxStream,
        lines: impl IntoIterator<Item = &'line str>,
    ) -> Result<SyntaxStreamUpdate, SyntaxStreamError> {
        let limit = self.highlighter.config.max_stream_bytes;
        let mut appended = String::new();
        for line in lines {
            let newline = usize::from(!line.ends_with('\n'));
            let attempted = stream.source.len() + appended.len() + line.len() + newline;
            if attempted > limit {
                return Err(SyntaxStreamError::InputLimit { limit, attempted });
            }
            appended.push_str(line);
            if newline != 0 {
                appended.push('\n');
            }
        }
        let base_revision = stream.revision;
        let theme_revision = self.theme.revision();
        if appended.is_empty() && stream.theme_revision == Some(theme_revision) {
            let end = stream.highlights.line_count();
            return Ok(SyntaxStreamUpdate {
                base_revision,
                revision: stream.revision,
                changed_lines: end..end,
                highlights: Arc::clone(&stream.highlights),
            });
        }
        if !appended.is_empty() {
            stream.source.push_str(&appended);
            stream.revision = stream.revision.wrapping_add(1);
        }
        self.highlighter.stats.calls += 1;
        let resolved = resolve_language(stream.hint.as_str(), &stream.source);
        let highlights = Arc::new(self.project(resolved, &stream.source));
        let first_changed = stream
            .highlights
            .lines
            .iter()
            .zip(&highlights.lines)
            .position(|(before, after)| before != after)
            .unwrap_or_else(|| stream.highlights.line_count().min(highlights.line_count()));
        stream.highlights = Arc::clone(&highlights);
        stream.theme_revision = Some(theme_revision);
        Ok(SyntaxStreamUpdate {
            base_revision,
            revision: stream.revision,
            changed_lines: first_changed..highlights.line_count(),
            highlights,
        })
    }
}

fn highlight_source(
    highlighter: &mut Highlighter,
    theme: &SyntaxTheme,
    language: &str,
    source: &str,
) -> Option<Vec<HighlightSpan>> {
    let raw_spans = highlighter.highlight_spans(language, source).ok()?;
    let tokens = spans_to_flat_tokens(source, raw_spans);
    let mut spans = Vec::with_capacity(tokens.len());
    for token in tokens {
        let Ok(start) = usize::try_from(token.start) else {
            continue;
        };
        let Ok(end) = usize::try_from(token.end) else {
            continue;
        };
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
        push_merged(
            &mut spans,
            HighlightSpan {
                range: start..end,
                foreground: style.foreground,
                font_style: style.font_style,
            },
        );
    }
    Some(spans)
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

fn push_merged(spans: &mut Vec<HighlightSpan>, span: HighlightSpan) {
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

struct JoinedLines<'a> {
    source: String,
    /// Sequence index, original line, global display start, global display end.
    lines: Vec<(usize, &'a str, usize, usize)>,
}

impl<'a> JoinedLines<'a> {
    fn new(selected: Vec<(usize, &'a str)>) -> Self {
        let mut source = String::new();
        let mut lines = Vec::with_capacity(selected.len());
        for (index, line) in selected {
            let start = source.len();
            source.push_str(line);
            let end = source.len();
            if !line.ends_with('\n') {
                source.push('\n');
            }
            lines.push((index, line, start, end));
        }
        Self { source, lines }
    }

    /// Splits window-global spans into per-line local spans. Spans must be
    /// disjoint and ordered, as [`highlight_source`] produces them, so one
    /// forward sweep serves every line; a span crossing lines is revisited
    /// only by the lines it overlaps.
    fn split(&self, spans: &[HighlightSpan]) -> Vec<Vec<HighlightSpan>> {
        let mut next = 0;
        self.lines
            .iter()
            .map(|(_, line, start, end)| {
                while spans.get(next).is_some_and(|span| span.range.end <= *start) {
                    next += 1;
                }
                let mut result = Vec::new();
                for span in &spans[next..] {
                    if span.range.start >= *end {
                        break;
                    }
                    let overlap_start = span.range.start.max(*start);
                    let overlap_end = span.range.end.min(*end);
                    if overlap_start >= overlap_end {
                        continue;
                    }
                    let local_start = overlap_start - start;
                    let local_end = overlap_end - start;
                    if local_end <= line.len()
                        && line.is_char_boundary(local_start)
                        && line.is_char_boundary(local_end)
                    {
                        push_merged(
                            &mut result,
                            HighlightSpan {
                                range: local_start..local_end,
                                foreground: span.foreground,
                                font_style: span.font_style,
                            },
                        );
                    }
                }
                result
            })
            .collect()
    }
}
