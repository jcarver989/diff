//! Tree-sitter syntax highlighting with UTF-8 byte spans and a bounded cache.

use crate::language::{LanguageHint, resolve_language};
use arborium::{Config, Highlighter};
use arborium_highlight::spans_to_flat_tokens;
use arborium_theme::tag_to_name;
use diff_fingerprint::SourceSequenceId;
use diff_theme::{DiffTheme, Fingerprint, HighlightSpan, SyntaxTheme};
use std::{
    collections::{HashMap, VecDeque},
    fmt,
    sync::{Arc, OnceLock},
};
const DEFAULT_CAPACITY: usize = 512;
const DEFAULT_MAX_DOCUMENTS: usize = 32;
const DEFAULT_STREAM_LINES: usize = 1_024;
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
    pub max_stream_lines: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: DEFAULT_CAPACITY,
            max_documents: DEFAULT_MAX_DOCUMENTS,
            max_stream_lines: DEFAULT_STREAM_LINES,
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

/// Opaque bounded continuation state for an append-only source stream.
#[derive(Debug, Clone, Default)]
pub struct SyntaxStream {
    hint: String,
    lines: Vec<String>,
}

impl SyntaxStream {
    #[must_use]
    pub fn new<'a>(hint: impl Into<LanguageHint<'a>>) -> Self {
        Self {
            hint: hint.into().as_str().to_owned(),
            lines: Vec::new(),
        }
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
}

/// A reusable syntax highlighter. Entries are evicted oldest-first when full.
pub struct SyntaxHighlighter {
    highlighter: Highlighter,
    config: CacheConfig,
    cache: HashMap<CacheKey, Arc<[HighlightSpan]>>,
    order: VecDeque<CacheKey>,
    documents: HashMap<CacheKey, Arc<DocumentHighlights>>,
    document_order: VecDeque<CacheKey>,
    stats: HighlightStats,
}

impl fmt::Debug for SyntaxHighlighter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyntaxHighlighter")
            .field("config", &self.config)
            .field("entries", &self.cache.len())
            .field("documents", &self.documents.len())
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
        Self {
            highlighter: Highlighter::with_config(syntax_config),
            config: config.into(),
            cache: HashMap::new(),
            order: VecDeque::new(),
            documents: HashMap::new(),
            document_order: VecDeque::new(),
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

    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.order.clear();
        self.documents.clear();
        self.document_order.clear();
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
        if let Some(spans) = self.cache.get(&key) {
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

    fn append<'line>(
        &mut self,
        theme: &SyntaxTheme,
        stream: &mut SyntaxStream,
        lines: impl IntoIterator<Item = &'line str>,
    ) -> Vec<Vec<HighlightSpan>> {
        let appended = lines.into_iter().map(str::to_owned).collect::<Vec<_>>();
        if appended.is_empty() {
            return Vec::new();
        }
        let previous_len = stream.lines.len();
        stream.lines.extend(appended);
        let first = previous_len.saturating_sub(self.config.max_stream_lines);
        let target_offset = previous_len - first;
        let mut highlighted = self.highlight_lines(
            theme,
            LanguageHint::Id(stream.hint.as_str()),
            stream.lines[first..].iter().map(String::as_str),
        );
        let result = highlighted.split_off(target_offset);
        if stream.lines.len() > self.config.max_stream_lines {
            let discard = stream.lines.len() - self.config.max_stream_lines;
            stream.lines.drain(..discard);
        }
        result
    }

    fn store(&mut self, key: CacheKey, spans: Arc<[HighlightSpan]>) {
        if self.config.max_entries == 0 {
            return;
        }
        if self.cache.insert(key, spans).is_some() {
            return;
        }
        self.order.push_back(key);
        while self.cache.len() > self.config.max_entries {
            let Some(evicted) = self.order.pop_front() else {
                break;
            };
            if self.cache.remove(&evicted).is_some() {
                self.stats.evictions += 1;
            }
        }
    }

    fn store_document(&mut self, key: CacheKey, highlights: Arc<DocumentHighlights>) {
        if self.config.max_entries == 0 || self.config.max_documents == 0 {
            return;
        }
        if self.documents.insert(key, highlights).is_some() {
            return;
        }
        self.document_order.push_back(key);
        while self.documents.len() > self.config.max_documents {
            let Some(evicted) = self.document_order.pop_front() else {
                break;
            };
            self.documents.remove(&evicted);
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
        if let Some(highlights) = self.highlighter.documents.get(&key) {
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
        let resolved = resolve_language(language.into(), "");
        let key = CacheKey::document(self.theme.revision(), resolved.unwrap_or("plain"), sequence);
        if let Some(highlights) = self.highlighter.documents.get(&key) {
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
        let spans = resolved
            .and_then(|language| {
                self.highlighter.stats.bytes =
                    self.highlighter.stats.bytes.saturating_add(text.len());
                highlight_source(
                    &mut self.highlighter.highlighter,
                    self.theme,
                    language,
                    text,
                )
            })
            .unwrap_or_default();
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
        let highlights = Arc::new(DocumentHighlights { lines });
        self.highlighter
            .store_document(key, Arc::clone(&highlights));
        highlights
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

    /// Appends lines to a stream and returns their spans in append order.
    /// Only bounded preceding context is retained between calls.
    pub fn append<'line>(
        &mut self,
        stream: &mut SyntaxStream,
        lines: impl IntoIterator<Item = &'line str>,
    ) -> Vec<Vec<HighlightSpan>> {
        self.highlighter.append(self.theme, stream, lines)
    }
}

fn highlight_source(
    highlighter: &mut Highlighter,
    theme: &DiffTheme,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_valid_spans(source: &str, spans: &[HighlightSpan]) {
        let mut previous_end = 0;
        for span in spans {
            assert!(span.range.start < span.range.end, "empty span: {span:?}");
            assert!(
                span.range.end <= source.len(),
                "out-of-bounds span: {span:?}"
            );
            assert!(source.is_char_boundary(span.range.start));
            assert!(source.is_char_boundary(span.range.end));
            assert!(
                span.range.start >= previous_end,
                "overlapping or unordered span: {span:?}"
            );
            previous_end = span.range.end;
        }
    }

    #[test]
    fn aliases_and_utf8_ranges() {
        let theme = DiffTheme::default();
        let mut highlighter = SyntaxHighlighter::new(2);
        let source = "let café = 1;\n";
        let spans = highlighter
            .with_theme(&theme)
            .highlight_source("rs", source);
        assert!(!spans.is_empty());
        assert!(
            spans
                .iter()
                .all(|span| source.is_char_boundary(span.range.start)
                    && source.is_char_boundary(span.range.end))
        );
        let _ = highlighter
            .with_theme(&theme)
            .highlight_source("RUST", source);
        assert_eq!(highlighter.stats().hits, 1);
    }

    #[test]
    fn complete_documents_are_parsed_once_and_projected_by_line() {
        let theme = DiffTheme::default();
        let mut highlighter = SyntaxHighlighter::default();
        let source = "/* alpha\r\nbeta */\nlet café = 1;\n";
        let sequence = SourceSequenceId::from_lines(source.split_terminator('\n'));

        let first = highlighter.with_theme(&theme).highlight_document(
            sequence,
            LanguageHint::Path("src/lib.rs"),
            source,
        );
        assert_eq!(first.line_count(), 3);
        assert!(!first.line(0).unwrap().is_empty());
        assert!(!first.line(1).unwrap().is_empty());
        assert!(
            first
                .line(2)
                .unwrap()
                .iter()
                .all(|span| span.range.end <= "let café = 1;".len())
        );
        let parsed_bytes = highlighter.stats().bytes;

        let second = highlighter.with_theme(&theme).highlight_document(
            sequence,
            LanguageHint::Path("src/lib.rs"),
            source,
        );
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(highlighter.stats().hits, 1);
        assert_eq!(highlighter.stats().bytes, parsed_bytes);

        let empty = highlighter.with_theme(&theme).highlight_document(
            SourceSequenceId::from_lines([]),
            "rust",
            "",
        );
        assert_eq!(empty.line_count(), 0);
    }

    #[test]
    fn line_sequences_parse_once_and_keep_multiline_context() {
        let theme = DiffTheme::default();
        let mut highlighter = SyntaxHighlighter::default();
        let lines = ["/* alpha", "beta", "gamma */", "let x = 1;"];
        let sequence = SourceSequenceId::from_lines(lines);

        let first = highlighter.with_theme(&theme).highlight_document_lines(
            sequence,
            LanguageHint::Path("src/lib.rs"),
            lines,
        );
        assert_eq!(first.line_count(), 4);
        for (index, line) in lines.iter().enumerate() {
            let spans = first.line(index).unwrap();
            assert!(!spans.is_empty(), "line {index}");
            assert!(spans.iter().all(|span| span.range.end <= line.len()));
        }

        let second = highlighter.with_theme(&theme).highlight_document_lines(
            sequence,
            LanguageHint::Path("src/lib.rs"),
            lines,
        );
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(highlighter.stats().misses, 1);
        assert_eq!(highlighter.stats().hits, 1);
    }

    #[test]
    fn supported_language_bundle_highlights_representative_sources() {
        let cases = [
            ("rust", "fn main() {}"),
            ("js", "const x = true;"),
            ("jsx", "const view = <Panel title=\"Hi\" />;"),
            ("typescript", "const x: number = 1;"),
            ("tsx", "const view = <Panel title=\"Hi\" />;"),
            ("py", "def f(): return 1"),
            ("sh", "echo hi"),
            ("c", "int main(void) {}"),
            ("cpp", "class C {};"),
            ("go", "package main"),
            ("json", "{\"x\": true}"),
            ("jsonc", "{\"x\": true /* comment */}"),
            ("toml", "x = 1"),
            ("yml", "x: true"),
            ("html", "<b>x</b>"),
            ("css", "b { color: red; }"),
            ("md", "# Heading"),
        ];
        let theme = DiffTheme::default();
        let mut highlighter = SyntaxHighlighter::new(cases.len());
        for (language, source) in cases {
            let spans = highlighter
                .with_theme(&theme)
                .highlight_source(language, source);
            assert!(!spans.is_empty(), "{language}");
            assert_valid_spans(source, &spans);
        }
    }

    #[test]
    fn unknown_language_is_plain_text_and_cached() {
        let mut highlighter = SyntaxHighlighter::new(2);
        let theme = DiffTheme::default();
        assert!(
            highlighter
                .with_theme(&theme)
                .highlight_source("binary.zzz", "abc")
                .is_empty()
        );
        assert!(
            highlighter
                .with_theme(&theme)
                .highlight_source("binary.zzz", "abc")
                .is_empty()
        );
        assert_eq!(highlighter.stats().hits, 1);
        assert_eq!(highlighter.stats().bytes, 0);
    }

    #[test]
    fn fifo_zero_capacity_and_theme_revision_behave_as_before() {
        let theme = DiffTheme::default();
        let mut fifo = SyntaxHighlighter::new(1);
        fifo.with_theme(&theme)
            .highlight_source("rust", "fn a() {}");
        fifo.with_theme(&theme)
            .highlight_source("rust", "fn b() {}");
        assert_eq!(fifo.stats().evictions, 1);
        let mut zero = SyntaxHighlighter::new(0);
        zero.with_theme(&theme)
            .highlight_source("rust", "fn a() {}");
        zero.with_theme(&theme)
            .highlight_source("rust", "fn a() {}");
        assert_eq!(zero.stats().misses, 2);
        let mut themes = SyntaxHighlighter::new(8);
        themes
            .with_theme(&theme)
            .highlight_source("rust", "fn main() {}");
        themes
            .with_theme(&DiffTheme::ayu().unwrap())
            .highlight_source("rust", "fn main() {}");
        assert_eq!(themes.stats().misses, 2);
    }

    #[test]
    fn multiline_and_synthetic_newlines_are_clipped() {
        let theme = DiffTheme::default();
        let mut highlighter = SyntaxHighlighter::new(0);
        let lines = ["/*", " café */"];
        let rendered = highlighter
            .with_theme(&theme)
            .highlight_lines("rust", lines);
        assert_eq!(rendered.len(), 2);
        assert!(rendered.iter().all(|spans| !spans.is_empty()));
        for (line, spans) in lines.into_iter().zip(rendered) {
            assert!(spans.iter().all(|span| span.range.end <= line.len()
                && line.is_char_boundary(span.range.start)
                && line.is_char_boundary(span.range.end)));
        }
    }

    #[test]
    fn html_and_markdown_injections_highlight_embedded_languages() {
        let theme = DiffTheme::default();
        let mut highlighter = SyntaxHighlighter::new(4);
        let html = "<p>café</p><script>const π = 3.14;</script><style>b{color:red}</style>";
        let html_spans = highlighter
            .with_theme(&theme)
            .highlight_source("html", html);
        assert_valid_spans(html, &html_spans);
        for needle in ["const π", "color:red"] {
            let start = html.find(needle).unwrap();
            let end = start + needle.len();
            assert!(
                html_spans
                    .iter()
                    .any(|span| span.range.start < end && span.range.end > start),
                "no injected highlight for {needle}"
            );
        }

        let markdown = "# Title\n\n```rust\nfn main() {}\n```\n";
        let markdown_spans = highlighter
            .with_theme(&theme)
            .highlight_source("markdown", markdown);
        assert_valid_spans(markdown, &markdown_spans);
        let start = markdown.find("fn main").unwrap();
        let end = start + "fn main".len();
        assert!(
            markdown_spans
                .iter()
                .any(|span| span.range.start < end && span.range.end > start),
            "no injected Rust highlight in Markdown"
        );
    }
}
