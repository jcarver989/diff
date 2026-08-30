//! Tree-sitter syntax highlighting with UTF-8 byte spans and a bounded cache.

use crate::language::{LanguageHint, resolve_language};
use arborium::{Config, Highlighter};
use arborium_highlight::spans_to_flat_tokens;
use arborium_theme::tag_to_name;
use diff_fingerprint::SourceSequenceId;
use diff_theme::{DiffTheme, Fingerprint, HighlightSpan, SyntaxTheme};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt,
    sync::{Arc, OnceLock},
};
const DEFAULT_CAPACITY: usize = 512;
const DEFAULT_MAX_SEQUENCES: usize = 32;
/// Maximum preceding lines parsed to reconstruct multiline state after a jump.
pub const PARSE_CONTEXT_LINES: usize = 1_024;
const MAX_PREFETCH_LINES: usize = 4_096;
const SOURCE_KEY_DOMAIN: &[u8] = b"syntax-source-v1";
const SEQUENCE_KEY_DOMAIN: &[u8] = b"syntax-sequence-line-v1";

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
    sequence: Option<Fingerprint>,
}

impl CacheKey {
    #[must_use]
    pub const fn fingerprint(self) -> Fingerprint {
        self.fingerprint
    }

    fn source(theme: Fingerprint, language: &str, source: &str) -> Self {
        Self::new(
            [
                SOURCE_KEY_DOMAIN,
                theme.as_bytes().as_slice(),
                language.as_bytes(),
                source.as_bytes(),
            ],
            None,
        )
    }

    fn sequence_line(
        theme: Fingerprint,
        language: &str,
        sequence: SourceSequenceId,
        line: usize,
    ) -> Self {
        let sequence = Fingerprint::from(sequence);
        let line = u64::try_from(line).unwrap_or(u64::MAX).to_le_bytes();
        Self::new(
            [
                SEQUENCE_KEY_DOMAIN,
                theme.as_bytes().as_slice(),
                language.as_bytes(),
                sequence.as_bytes().as_slice(),
                line.as_slice(),
            ],
            Some(sequence),
        )
    }

    fn new<const N: usize>(fields: [&[u8]; N], sequence: Option<Fingerprint>) -> Self {
        Self {
            fingerprint: Fingerprint::of(fields),
            sequence,
        }
    }
}

/// Fixed resource limits for a [`SyntaxHighlighter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheConfig {
    pub max_entries: usize,
    pub max_sequences: usize,
    pub context_lines: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: DEFAULT_CAPACITY,
            max_sequences: DEFAULT_MAX_SEQUENCES,
            context_lines: PARSE_CONTEXT_LINES,
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

/// One random-access line within a content-identified source sequence.
pub struct SequenceLine<'a, T> {
    language: LanguageHint<'a>,
    sequence: SourceSequenceId,
    target_line: usize,
    lines: T,
}

impl<'a, T> SequenceLine<'a, T> {
    #[must_use]
    pub fn new(
        sequence: SourceSequenceId,
        language: impl Into<LanguageHint<'a>>,
        target_line: usize,
        lines: T,
    ) -> Self {
        Self {
            language: language.into(),
            sequence,
            target_line,
            lines,
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

/// A reusable syntax highlighter. Entries are evicted oldest-first when full.
pub struct SyntaxHighlighter {
    highlighter: Highlighter,
    config: CacheConfig,
    prefetch_lines: usize,
    cache: HashMap<CacheKey, Arc<[HighlightSpan]>>,
    order: VecDeque<CacheKey>,
    sequences: HashSet<Fingerprint>,
    sequence_order: VecDeque<Fingerprint>,
    stats: HighlightStats,
}

impl fmt::Debug for SyntaxHighlighter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyntaxHighlighter")
            .field("config", &self.config)
            .field("prefetch_lines", &self.prefetch_lines)
            .field("entries", &self.cache.len())
            .field("sequences", &self.sequences.len())
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
            prefetch_lines: 0,
            cache: HashMap::new(),
            order: VecDeque::new(),
            sequences: HashSet::new(),
            sequence_order: VecDeque::new(),
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

    /// Sets bounded lookahead for subsequent sequence requests without changing
    /// the configured cache budget.
    pub fn prepare_viewport(&mut self, rows: usize) {
        self.prefetch_lines = rows.saturating_mul(2).min(MAX_PREFETCH_LINES);
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
        self.sequences.clear();
        self.sequence_order.clear();
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

    fn highlight_line<'line, T>(
        &mut self,
        theme: &SyntaxTheme,
        request: SequenceLine<'_, T>,
    ) -> Arc<[HighlightSpan]>
    where
        T: IntoIterator<Item = &'line str>,
    {
        self.stats.calls += 1;
        let language = resolve_language(request.language, "");
        let id = language.unwrap_or("plain");
        let target_key =
            CacheKey::sequence_line(theme.revision(), id, request.sequence, request.target_line);
        if let Some(spans) = self.cache.get(&target_key) {
            self.stats.hits += 1;
            return Arc::clone(spans);
        }
        self.stats.misses += 1;
        let Some(language) = language else {
            let spans = empty_spans();
            self.store(target_key, Arc::clone(&spans));
            return spans;
        };

        self.reserve_sequence(request.sequence.fingerprint());
        let first = request
            .target_line
            .saturating_sub(self.config.context_lines);
        let last = request.target_line.saturating_add(self.prefetch_lines);
        let selected: Vec<(usize, &str)> = request
            .lines
            .into_iter()
            .enumerate()
            .skip(first)
            .take(last.saturating_sub(first).saturating_add(1))
            .collect();
        if selected
            .iter()
            .all(|(index, _)| *index != request.target_line)
        {
            return empty_spans();
        }

        let window = SourceWindow::new(selected);
        self.stats.bytes = self.stats.bytes.saturating_add(window.source.len());
        let global = highlight_source(&mut self.highlighter, theme, language, &window.source);
        let per_line = global.as_deref().map_or_else(
            || vec![Vec::new(); window.lines.len()],
            |spans| window.split(spans),
        );
        let mut target_spans = None;
        let mut prefetched = Vec::new();
        let cache_sequences = self.config.max_sequences != 0;
        for ((line, _, _, _), spans) in window.lines.iter().zip(per_line) {
            let spans: Arc<[HighlightSpan]> = Arc::from(spans);
            if *line == request.target_line {
                target_spans = Some(spans);
            } else if *line > request.target_line && cache_sequences {
                prefetched.push((
                    CacheKey::sequence_line(theme.revision(), id, request.sequence, *line),
                    spans,
                ));
            }
        }
        // Eviction is FIFO by insertion order, so store the prefetch window
        // farthest line first and the requested line last: lookahead larger
        // than the cache budget sheds itself before the line the caller needs.
        for (key, spans) in prefetched.into_iter().rev() {
            self.store(key, spans);
        }
        let target_spans = target_spans.unwrap_or_else(empty_spans);
        if cache_sequences {
            self.store(target_key, Arc::clone(&target_spans));
        }
        if self.config.max_entries == 0 {
            self.sequences.remove(&request.sequence.fingerprint());
        }
        target_spans
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
        let window = SourceWindow::new(selected);
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
        let first = previous_len.saturating_sub(self.config.context_lines);
        let target_offset = previous_len - first;
        let mut highlighted = self.highlight_lines(
            theme,
            LanguageHint::Id(stream.hint.as_str()),
            stream.lines[first..].iter().map(String::as_str),
        );
        let result = highlighted.split_off(target_offset);
        if stream.lines.len() > self.config.context_lines {
            let discard = stream.lines.len() - self.config.context_lines;
            stream.lines.drain(..discard);
        }
        result
    }

    fn reserve_sequence(&mut self, key: Fingerprint) {
        if self.config.max_entries == 0
            || self.config.max_sequences == 0
            || self.sequences.contains(&key)
        {
            return;
        }
        while self.sequences.len() >= self.config.max_sequences {
            let Some(evicted) = self.sequence_order.pop_front() else {
                break;
            };
            self.sequences.remove(&evicted);
            let before = self.cache.len();
            self.cache.retain(|key, _| key.sequence != Some(evicted));
            self.order.retain(|key| key.sequence != Some(evicted));
            self.stats.evictions = self
                .stats
                .evictions
                .saturating_add(u64::try_from(before - self.cache.len()).unwrap_or(u64::MAX));
        }
        self.sequences.insert(key);
        self.sequence_order.push_back(key);
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
}

/// Theme-bound highlighting operations.
pub struct ThemedHighlighter<'a> {
    highlighter: &'a mut SyntaxHighlighter,
    theme: &'a SyntaxTheme,
}

impl ThemedHighlighter<'_> {
    /// Highlights a complete source. Hints may be IDs, aliases, or repository paths.
    pub fn highlight_source<'a>(
        &mut self,
        language: impl Into<LanguageHint<'a>>,
        text: &str,
    ) -> Arc<[HighlightSpan]> {
        self.highlighter
            .highlight_source(self.theme, language.into(), text)
    }

    /// Highlights one sequence line using bounded preceding context and viewport prefetch.
    pub fn highlight_line<'line, T>(&mut self, request: SequenceLine<'_, T>) -> Arc<[HighlightSpan]>
    where
        T: IntoIterator<Item = &'line str>,
    {
        self.highlighter.highlight_line(self.theme, request)
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

struct SourceWindow<'a> {
    source: String,
    /// Sequence index, original line, global display start, global display end.
    lines: Vec<(usize, &'a str, usize, usize)>,
}

impl<'a> SourceWindow<'a> {
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
    fn sequence_work_is_context_bounded_and_prefetches() {
        let owned: Vec<String> = (0..3_000)
            .map(|i| format!("let value_{i} = {i};"))
            .collect();
        let lines: Vec<&str> = owned.iter().map(String::as_str).collect();
        let sequence = SourceSequenceId::from_lines(lines.iter().copied());
        let theme = DiffTheme::default();
        let mut highlighter = SyntaxHighlighter::new(64);
        highlighter.prepare_viewport(20);
        assert_eq!(highlighter.config().max_entries, 64);
        highlighter
            .with_theme(&theme)
            .highlight_line(SequenceLine::new(
                sequence,
                "src/lib.rs",
                2_500,
                lines.iter().copied(),
            ));
        let bytes = highlighter.stats().bytes;
        let full: usize = lines.iter().map(|line| line.len() + 1).sum();
        assert!(bytes < full / 2, "parsed {bytes} of {full} bytes");
        highlighter
            .with_theme(&theme)
            .highlight_line(SequenceLine::new(
                sequence,
                "src/lib.rs",
                2_510,
                lines.iter().copied(),
            ));
        assert_eq!(
            highlighter.stats().hits,
            1,
            "prefetched viewport line should hit"
        );
    }

    #[test]
    fn requested_line_survives_prefetch_larger_than_the_cache_budget() {
        let lines = ["let a = 1;", "let b = 2;", "let c = 3;", "let d = 4;"];
        let sequence = SourceSequenceId::from_lines(lines);
        let theme = DiffTheme::default();
        let mut highlighter = SyntaxHighlighter::new(2);
        highlighter.prepare_viewport(20);

        for _ in 0..2 {
            highlighter
                .with_theme(&theme)
                .highlight_line(SequenceLine::new(
                    sequence,
                    "src/lib.rs",
                    0,
                    lines.iter().copied(),
                ));
        }

        assert_eq!(highlighter.config().max_entries, 2);
        assert_eq!(highlighter.stats().hits, 1);
    }

    #[test]
    fn split_view_sequences_share_the_cache_without_evicting_each_other() {
        let owned: Vec<String> = (0..1_100).map(|i| format!("let v{i} = {i};")).collect();
        let lines: Vec<&str> = owned.iter().map(String::as_str).collect();
        let theme = DiffTheme::default();
        let mut highlighter = SyntaxHighlighter::default();
        highlighter.prepare_viewport(5);
        let sides = [
            SourceSequenceId::from_lines(std::iter::once("old")),
            SourceSequenceId::from_lines(std::iter::once("new")),
        ];
        for _frame in 0..3 {
            for row in 1_030..1_035 {
                for sequence in sides {
                    let spans = highlighter
                        .with_theme(&theme)
                        .highlight_line(SequenceLine::new(
                            sequence,
                            "src/lib.rs",
                            row,
                            lines.iter().copied(),
                        ));
                    assert!(!spans.is_empty(), "row {row}");
                }
            }
        }
        assert_eq!(
            highlighter.stats().misses,
            2,
            "one context parse per side, then every cell hits"
        );
    }

    #[test]
    fn rerendered_frames_stay_cached_without_a_viewport_reservation() {
        // Hosts may render before ever calling `prepare_viewport`; with no
        // prefetch each row still misses once, but repeated frames over the
        // same viewport must not parse again.
        let owned: Vec<String> = (0..1_100).map(|i| format!("let v{i} = {i};")).collect();
        let lines: Vec<&str> = owned.iter().map(String::as_str).collect();
        let theme = DiffTheme::default();
        let mut highlighter = SyntaxHighlighter::default();
        let sides = [
            SourceSequenceId::from_lines(std::iter::once("old")),
            SourceSequenceId::from_lines(std::iter::once("new")),
        ];
        for _frame in 0..3 {
            for row in 1_030..1_035 {
                for sequence in sides {
                    let _ = highlighter
                        .with_theme(&theme)
                        .highlight_line(SequenceLine::new(
                            sequence,
                            "src/lib.rs",
                            row,
                            lines.iter().copied(),
                        ));
                }
            }
        }
        assert_eq!(highlighter.stats().misses, 10, "each cell parses only once");
    }

    #[test]
    fn multiline_spans_cover_every_line_they_cross() {
        let theme = DiffTheme::default();
        let mut highlighter = SyntaxHighlighter::default();
        let lines = ["/* alpha", "", "beta", "gamma */"];
        let sequence = SourceSequenceId::from_lines(lines);
        highlighter.prepare_viewport(lines.len());
        for (index, line) in lines.iter().enumerate() {
            let spans = highlighter
                .with_theme(&theme)
                .highlight_line(SequenceLine::new(
                    sequence,
                    "rs",
                    index,
                    lines.iter().copied(),
                ));
            if line.is_empty() {
                assert!(spans.is_empty(), "line {index}");
            } else {
                assert_eq!(spans.first().map(|span| span.range.start), Some(0));
                assert_eq!(spans.last().map(|span| span.range.end), Some(line.len()));
            }
        }
        assert_eq!(highlighter.stats().misses, 1, "one parse covers the window");
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
