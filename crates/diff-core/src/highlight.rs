//! Pure-Rust syntax highlighting with UTF-8 byte spans and a bounded cache.

use crate::{DiffTheme, Fingerprint, FontStyle, HighlightSpan, Rgba};
use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    fmt,
    sync::{Arc, OnceLock},
};
use syntect::{
    easy::HighlightLines,
    highlighting::{
        FontStyle as SyntectFontStyle, HighlightIterator, HighlightState, Highlighter, Style,
    },
    parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet},
};

/// Span sets a highlighter caches by default. One visible cell asks for one
/// entry, so this holds several viewports and switching files stays warm.
const DEFAULT_CAPACITY: usize = 512;

/// Cached span sets reserved per viewport row by
/// [`SyntaxHighlighter::reserve_for_viewport`].
const CAPACITY_PER_ROW: usize = 8;

/// Incremental parsers kept alive at once. A viewport spans few hunks, and each
/// parser retains its own snapshots.
const MAX_SEQUENCES: usize = 32;

/// Lines between the parser snapshots that let a backward jump rewind instead
/// of re-parsing a sequence from its first line.
const CHECKPOINT_INTERVAL: usize = 64;

/// Snapshots kept per sequence. Passing it halves the snapshots and doubles the
/// interval, so a long sequence stays bounded at the cost of coarser rewinds.
const MAX_CHECKPOINTS: usize = 64;

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
    pub bytes: usize,
}

fn syntax_set() -> &'static SyntaxSet {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(two_face::syntax::extra_newlines)
}

/// A reusable syntax highlighter. Entries are evicted oldest-first when full.
pub struct SyntaxHighlighter {
    syntax: &'static SyntaxSet,
    capacity: usize,
    cache: HashMap<Fingerprint, Arc<[HighlightSpan]>>,
    order: VecDeque<Fingerprint>,
    sequences: HashMap<Fingerprint, SequenceState>,
    sequence_order: VecDeque<Fingerprint>,
    stats: HighlightStats,
}

/// A parser position a sequence can rewind to, captured before its line is
/// parsed.
struct Checkpoint {
    line: usize,
    parse: ParseState,
    highlight: HighlightState,
}

struct SequenceState {
    parse: ParseState,
    highlight: HighlightState,
    next_line: usize,
    checkpoints: Vec<Checkpoint>,
    checkpoint_stride: usize,
}

impl SequenceState {
    fn new(syntax: &SyntaxReference, highlighter: &Highlighter<'_>) -> Self {
        Self {
            parse: ParseState::new(syntax),
            highlight: HighlightState::new(highlighter, ScopeStack::new()),
            next_line: 0,
            checkpoints: Vec::new(),
            checkpoint_stride: CHECKPOINT_INTERVAL,
        }
    }

    /// Moves the parser to a line at or before `target`. A parser already past
    /// it rewinds to the nearest snapshot, and restarts only when no snapshot
    /// covers the target.
    fn seek(&mut self, target: usize, syntax: &SyntaxReference, highlighter: &Highlighter<'_>) {
        if self.next_line <= target {
            return;
        }
        let Some(index) = self
            .checkpoints
            .iter()
            .rposition(|point| point.line <= target)
        else {
            self.parse = ParseState::new(syntax);
            self.highlight = HighlightState::new(highlighter, ScopeStack::new());
            self.next_line = 0;
            return;
        };
        let point = &self.checkpoints[index];
        self.parse = point.parse.clone();
        self.highlight = point.highlight.clone();
        self.next_line = point.line;
    }

    /// Snapshots the parser before `line` is parsed, when `line` sits on the
    /// current stride and no snapshot covers it yet.
    fn checkpoint(&mut self, line: usize) {
        if !line.is_multiple_of(self.checkpoint_stride)
            || self
                .checkpoints
                .last()
                .is_some_and(|point| point.line >= line)
        {
            return;
        }
        self.checkpoints.push(Checkpoint {
            line,
            parse: self.parse.clone(),
            highlight: self.highlight.clone(),
        });
        if self.checkpoints.len() <= MAX_CHECKPOINTS {
            return;
        }
        let mut thinned = Vec::with_capacity(self.checkpoints.len().div_ceil(2));
        for (index, point) in self.checkpoints.drain(..).enumerate() {
            if index % 2 == 0 {
                thinned.push(point);
            }
        }
        self.checkpoints = thinned;
        self.checkpoint_stride = self.checkpoint_stride.saturating_mul(2);
    }
}

impl fmt::Debug for SyntaxHighlighter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyntaxHighlighter")
            .field("capacity", &self.capacity)
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
    /// Creates a highlighter with at most `capacity` cached sources.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            syntax: syntax_set(),
            capacity,
            cache: HashMap::new(),
            order: VecDeque::new(),
            sequences: HashMap::new(),
            sequence_order: VecDeque::new(),
            stats: HighlightStats::default(),
        }
    }

    #[must_use]
    pub const fn stats(&self) -> HighlightStats {
        self.stats
    }

    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Grows the cache to hold what a `rows`-tall viewport asks for, so a
    /// viewport taller than [`DEFAULT_CAPACITY`] assumes does not evict the
    /// rows it is still drawing. The capacity never shrinks, and a highlighter
    /// built with capacity zero stays uncached, so hosts may call this every
    /// frame.
    pub fn reserve_for_viewport(&mut self, rows: usize) {
        if self.capacity == 0 {
            return;
        }
        self.capacity = self.capacity.max(rows.saturating_mul(CAPACITY_PER_ROW));
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.order.clear();
        self.sequences.clear();
        self.sequence_order.clear();
    }

    /// Highlights `source`, resolving common names, extensions, and aliases.
    pub fn highlight(
        &mut self,
        theme: &DiffTheme,
        language: &str,
        source: &str,
    ) -> Arc<[HighlightSpan]> {
        self.stats.calls += 1;
        let language = canonical_language(language);
        let revision = theme.revision();
        let key = Fingerprint::of([
            revision.as_bytes().as_slice(),
            language.as_bytes(),
            source.as_bytes(),
        ]);
        if let Some(spans) = self.cache.get(&key) {
            self.stats.hits += 1;
            return Arc::clone(spans);
        }
        self.stats.misses += 1;
        self.stats.bytes += source.len();
        let spans: Arc<[HighlightSpan]> =
            Arc::from(self.highlight_uncached(theme, &language, source));
        self.store(key, Arc::clone(&spans));
        spans
    }

    /// Highlights line `target` of `lines` with a parser that keeps its place
    /// between calls, so walking a hunk costs one line of work per line.
    pub fn highlight_in_sequence<'a, T>(
        &mut self,
        theme: &DiffTheme,
        language: &str,
        sequence: Fingerprint,
        target: usize,
        lines: T,
    ) -> Arc<[HighlightSpan]>
    where
        T: IntoIterator<Item = &'a str>,
    {
        self.stats.calls += 1;
        let language = canonical_language(language);
        let revision = theme.revision();
        let sequence_key = Fingerprint::of([
            revision.as_bytes().as_slice(),
            language.as_bytes(),
            sequence.as_bytes().as_slice(),
        ]);
        if let Some(spans) = self.cache.get(&sequence_line_key(sequence_key, target)) {
            self.stats.hits += 1;
            return Arc::clone(spans);
        }
        self.stats.misses += 1;

        let syntax = resolve_syntax(self.syntax, &language);
        let highlighter = Highlighter::new(theme.syntax());
        self.reserve_sequence(sequence_key);
        let state = self
            .sequences
            .entry(sequence_key)
            .or_insert_with(|| SequenceState::new(syntax, &highlighter));
        state.seek(target, syntax, &highlighter);

        let mut target_spans = None;
        let mut parsed = Vec::new();
        for (index, line) in lines.into_iter().enumerate().skip(state.next_line) {
            if index > target {
                break;
            }
            self.stats.bytes += line.len();
            state.checkpoint(index);
            let spans: Arc<[HighlightSpan]> = Arc::from(spans_for_state(
                &mut state.parse,
                &mut state.highlight,
                &highlighter,
                self.syntax,
                line,
            ));
            state.next_line = index.saturating_add(1);
            if index == target {
                target_spans = Some(Arc::clone(&spans));
            }
            parsed.push((sequence_line_key(sequence_key, index), spans));
        }
        for (key, spans) in parsed {
            self.store(key, spans);
        }
        if self.capacity == 0 {
            self.sequences.remove(&sequence_key);
        }
        target_spans.unwrap_or_else(empty_spans)
    }

    /// Highlights lines in order with one parser, preserving multiline state.
    #[must_use]
    pub fn highlight_sequential<'a, T>(
        &self,
        theme: &DiffTheme,
        language: &str,
        lines: T,
    ) -> Vec<Vec<HighlightSpan>>
    where
        T: IntoIterator<Item = &'a str>,
    {
        let syntax = resolve_syntax(self.syntax, language);
        let mut highlighter = HighlightLines::new(syntax, theme.syntax());
        lines
            .into_iter()
            .map(|line| spans_for_line(&mut highlighter, self.syntax, line))
            .collect()
    }

    /// Makes room for one more incremental parser, evicting the oldest.
    fn reserve_sequence(&mut self, key: Fingerprint) {
        if self.capacity == 0 || self.sequences.contains_key(&key) {
            return;
        }
        while self.sequences.len() >= MAX_SEQUENCES {
            let Some(evicted) = self.sequence_order.pop_front() else {
                break;
            };
            self.sequences.remove(&evicted);
        }
        self.sequence_order.push_back(key);
    }

    fn store(&mut self, key: Fingerprint, spans: Arc<[HighlightSpan]>) {
        if self.capacity == 0 {
            return;
        }
        // Refreshing an entry must not queue a second copy of its key, or the
        // eviction order drifts out of step with the cache and starts dropping
        // live entries early.
        if self.cache.insert(key, spans).is_some() {
            return;
        }
        self.order.push_back(key);
        while self.cache.len() > self.capacity {
            let Some(evicted) = self.order.pop_front() else {
                break;
            };
            if self.cache.remove(&evicted).is_some() {
                self.stats.evictions += 1;
            }
        }
    }

    fn highlight_uncached(
        &self,
        theme: &DiffTheme,
        language: &str,
        source: &str,
    ) -> Vec<HighlightSpan> {
        let syntax = resolve_syntax(self.syntax, language);
        let mut highlighter = HighlightLines::new(syntax, theme.syntax());
        let mut output = Vec::new();
        let mut offset = 0;
        for line in source.split_inclusive('\n') {
            for mut span in spans_for_line(&mut highlighter, self.syntax, line) {
                span.range.start += offset;
                span.range.end += offset;
                output.push(span);
            }
            offset += line.len();
        }
        output
    }
}

fn sequence_line_key(sequence: Fingerprint, line: usize) -> Fingerprint {
    Fingerprint::of([
        sequence.as_bytes().as_slice(),
        &u64::try_from(line).unwrap_or(u64::MAX).to_le_bytes(),
    ])
}

fn canonical_language(language: &str) -> Cow<'_, str> {
    let trimmed = language.trim();
    let lowercase = if trimmed.bytes().any(|byte| byte.is_ascii_uppercase()) {
        Cow::Owned(trimmed.to_ascii_lowercase())
    } else {
        Cow::Borrowed(trimmed)
    };
    match lowercase.as_ref() {
        "rs" => Cow::Borrowed("rust"),
        "js" => Cow::Borrowed("javascript"),
        "ts" => Cow::Borrowed("typescript"),
        "py" => Cow::Borrowed("python"),
        "md" => Cow::Borrowed("markdown"),
        "sh" | "shell" => Cow::Borrowed("bash"),
        _ => lowercase,
    }
}

fn resolve_syntax<'a>(set: &'a SyntaxSet, language: &str) -> &'a SyntaxReference {
    let canonical = canonical_language(language);
    set.find_syntax_by_token(&canonical)
        .or_else(|| set.find_syntax_by_extension(&canonical))
        .or_else(|| set.find_syntax_by_name(&canonical))
        .unwrap_or_else(|| set.find_syntax_plain_text())
}

fn spans_for_state(
    parse: &mut ParseState,
    state: &mut HighlightState,
    highlighter: &Highlighter<'_>,
    set: &SyntaxSet,
    line: &str,
) -> Vec<HighlightSpan> {
    let Ok(ops) = parse.parse_line(line, set) else {
        return fallback_span(line);
    };
    let parts: Vec<(Style, &str)> =
        HighlightIterator::new(state, &ops, line, highlighter).collect();
    spans_for_parts(parts, line)
}

fn spans_for_line(
    highlighter: &mut HighlightLines<'_>,
    set: &SyntaxSet,
    line: &str,
) -> Vec<HighlightSpan> {
    let Ok(parts) = highlighter.highlight_line(line, set) else {
        return fallback_span(line);
    };
    spans_for_parts(parts, line)
}

fn fallback_span(line: &str) -> Vec<HighlightSpan> {
    vec![HighlightSpan {
        range: 0..line.len(),
        foreground: Rgba::default(),
        font_style: FontStyle::none(),
    }]
}

fn spans_for_parts(parts: Vec<(Style, &str)>, _line: &str) -> Vec<HighlightSpan> {
    let mut offset = 0;
    let mut spans = Vec::with_capacity(parts.len());
    for (style, text) in parts {
        let end = offset + text.len();
        if end > offset {
            spans.push(HighlightSpan {
                range: offset..end,
                foreground: Rgba::new(
                    style.foreground.r,
                    style.foreground.g,
                    style.foreground.b,
                    style.foreground.a,
                ),
                font_style: font_style(style),
            });
        }
        offset = end;
    }
    spans
}

const fn font_style(style: Style) -> FontStyle {
    FontStyle {
        bold: style.font_style.contains(SyntectFontStyle::BOLD),
        italic: style.font_style.contains(SyntectFontStyle::ITALIC),
        underline: style.font_style.contains(SyntectFontStyle::UNDERLINE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_and_utf8_ranges() {
        let theme = DiffTheme::default();
        let mut highlighter = SyntaxHighlighter::new(2);
        let source = "let café = 1;\n";
        let spans = highlighter.highlight(&theme, "rs", source);
        assert!(spans.iter().all(|span| {
            source.is_char_boundary(span.range.start) && source.is_char_boundary(span.range.end)
        }));
        assert_eq!(highlighter.stats().misses, 1);
        let _ = highlighter.highlight(&theme, "RUST", source);
        assert_eq!(highlighter.stats().hits, 1);
    }

    #[test]
    fn fifo_eviction() {
        let theme = DiffTheme::default();
        let mut highlighter = SyntaxHighlighter::new(1);
        highlighter.highlight(&theme, "text", "a");
        highlighter.highlight(&theme, "text", "b");
        assert_eq!(highlighter.stats().evictions, 1);
    }

    #[test]
    fn a_zero_capacity_highlighter_never_caches() {
        let theme = DiffTheme::default();
        let mut highlighter = SyntaxHighlighter::new(0);
        highlighter.highlight(&theme, "text", "a");
        highlighter.highlight(&theme, "text", "a");
        assert_eq!(highlighter.stats().misses, 2);
        assert_eq!(highlighter.stats().hits, 0);
    }

    #[test]
    fn a_different_theme_invalidates_cached_spans() {
        let sage = DiffTheme::default();
        let ayu = DiffTheme::ayu().unwrap();
        let mut highlighter = SyntaxHighlighter::new(8);
        highlighter.highlight(&sage, "rust", "fn main() {}");
        highlighter.highlight(&ayu, "rust", "fn main() {}");
        assert_eq!(highlighter.stats().misses, 2);
    }

    #[test]
    fn syntax_definitions_are_shared_between_highlighters() {
        assert!(std::ptr::eq(
            SyntaxHighlighter::new(1).syntax,
            SyntaxHighlighter::new(2).syntax
        ));
    }

    #[test]
    fn a_backward_jump_rewinds_to_a_checkpoint_instead_of_the_first_line() {
        let theme = DiffTheme::default();
        let lines: Vec<String> = (0..600)
            .map(|line| format!("let value_{line} = {line};\n"))
            .collect();
        let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
        let sequence = Fingerprint::of([b"sequence".as_slice()]);
        // A cache too small to keep the earlier line forces the re-parse this
        // test is about.
        let mut highlighter = SyntaxHighlighter::new(4);

        highlighter.highlight_in_sequence(&theme, "rust", sequence, 599, borrowed.iter().copied());
        let warm = highlighter.stats().bytes;
        highlighter.highlight_in_sequence(&theme, "rust", sequence, 500, borrowed.iter().copied());
        let reparsed = highlighter.stats().bytes - warm;

        let prefix: usize = borrowed[..=500].iter().map(|line| line.len()).sum();
        assert!(reparsed > 0, "line 500 should have been evicted");
        assert!(
            reparsed < prefix / 4,
            "rewound {reparsed} bytes of a {prefix}-byte prefix"
        );
    }

    #[test]
    fn the_cache_grows_for_a_tall_viewport_but_never_shrinks() {
        let mut highlighter = SyntaxHighlighter::default();
        let default = highlighter.capacity();
        highlighter.reserve_for_viewport(10);
        assert_eq!(highlighter.capacity(), default);

        highlighter.reserve_for_viewport(400);
        let grown = highlighter.capacity();
        assert!(grown > default);
        highlighter.reserve_for_viewport(1);
        assert_eq!(highlighter.capacity(), grown);

        let mut uncached = SyntaxHighlighter::new(0);
        uncached.reserve_for_viewport(400);
        assert_eq!(uncached.capacity(), 0, "capacity zero stays uncached");
    }

    #[test]
    fn multiline_is_sequential() {
        let theme = DiffTheme::default();
        let highlighter = SyntaxHighlighter::new(0);
        let spans = highlighter.highlight_sequential(&theme, "rust", ["/*", " comment */"]);
        assert_eq!(spans.len(), 2);
    }
}
