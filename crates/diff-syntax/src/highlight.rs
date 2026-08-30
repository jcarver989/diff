//! Tree-sitter syntax highlighting with UTF-8 byte spans and a bounded cache.

use crate::language::{LanguageHint, resolve_language};
use arborium::{Config, Highlighter};
use arborium_highlight::spans_to_flat_tokens;
use arborium_theme::tag_to_name;
use diff_theme::{DiffTheme, Fingerprint, HighlightSpan, SyntaxTheme};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt,
    sync::{Arc, OnceLock},
};
const DEFAULT_CAPACITY: usize = 512;
const CAPACITY_PER_ROW: usize = 8;
const MAX_SEQUENCES: usize = 32;
/// Maximum preceding lines parsed to reconstruct multiline state after a jump.
pub const PARSE_CONTEXT_LINES: usize = 1_024;
const MAX_PREFETCH_LINES: usize = 4_096;

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

/// Opaque bounded continuation state for append-only line streams.
///
/// A sequence retains only bounded source lines and reparses that window on
/// each append, so callers never depend on Arborium state.
#[derive(Debug, Clone, Default)]
pub struct SyntaxSequence {
    hint: String,
    lines: Vec<String>,
}

impl SyntaxSequence {
    /// Starts an empty sequence for one logical stream of lines.
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
    capacity: usize,
    prefetch_lines: usize,
    cache: HashMap<Fingerprint, Arc<[HighlightSpan]>>,
    order: VecDeque<Fingerprint>,
    sequences: HashSet<Fingerprint>,
    sequence_order: VecDeque<Fingerprint>,
    stats: HighlightStats,
}

impl fmt::Debug for SyntaxHighlighter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyntaxHighlighter")
            .field("capacity", &self.capacity)
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
    /// Creates a highlighter with at most `capacity` cached sources.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let config = Config {
            max_injection_depth: 3,
            ..Config::default()
        };
        Self {
            highlighter: Highlighter::with_config(config),
            capacity,
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

    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Reserves cache space and records bounded sequence prefetch for a
    /// viewport. Prefetch spans two viewports because after a jump the miss
    /// lands on the top visible row: one viewport reaches the bottom of the
    /// screen and the second is lookahead for the scroll that follows.
    pub fn reserve_for_viewport(&mut self, rows: usize) {
        self.prefetch_lines = rows.saturating_mul(2).min(MAX_PREFETCH_LINES);
        if self.capacity != 0 {
            self.capacity = self.capacity.max(rows.saturating_mul(CAPACITY_PER_ROW));
        }
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.order.clear();
        self.sequences.clear();
        self.sequence_order.clear();
    }

    /// Highlights a complete source. Hints may be IDs, aliases, or repository paths.
    pub fn highlight<'a>(
        &mut self,
        theme: &SyntaxTheme,
        hint: impl Into<LanguageHint<'a>>,
        source: &str,
    ) -> Arc<[HighlightSpan]> {
        self.stats.calls += 1;
        let language = resolve_language(hint, source);
        let id = language.unwrap_or("plain");
        let key = Fingerprint::of([
            theme.revision().as_bytes().as_slice(),
            id.as_bytes(),
            source.as_bytes(),
        ]);
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
        self.stats.bytes = self.stats.bytes.saturating_add(source.len());
        let spans = highlight_source(&mut self.highlighter, theme, language, source)
            .map_or_else(empty_spans, Arc::from);
        self.store(key, Arc::clone(&spans));
        spans
    }

    /// Highlights one sequence line using bounded preceding context and viewport prefetch.
    /// Constructs beginning more than [`PARSE_CONTEXT_LINES`] earlier rely on parser recovery.
    pub fn highlight_in_sequence<'a, 'hint, T>(
        &mut self,
        theme: &SyntaxTheme,
        hint: impl Into<LanguageHint<'hint>>,
        sequence: Fingerprint,
        target: usize,
        lines: T,
    ) -> Arc<[HighlightSpan]>
    where
        T: IntoIterator<Item = &'a str>,
    {
        self.stats.calls += 1;
        let language = resolve_language(hint, "");
        let id = language.unwrap_or("plain");
        let sequence_key = Fingerprint::of([
            theme.revision().as_bytes().as_slice(),
            id.as_bytes(),
            sequence.as_bytes().as_slice(),
        ]);
        let target_key = sequence_line_key(sequence_key, target);
        if let Some(spans) = self.cache.get(&target_key) {
            self.stats.hits += 1;
            return Arc::clone(spans);
        }
        self.stats.misses += 1;
        let Some(language) = language else {
            return empty_spans();
        };

        self.reserve_sequence(sequence_key);
        let first = target.saturating_sub(PARSE_CONTEXT_LINES);
        let last = target.saturating_add(self.prefetch_lines);
        let selected: Vec<(usize, &str)> = lines
            .into_iter()
            .enumerate()
            .skip(first)
            .take(last.saturating_sub(first).saturating_add(1))
            .collect();
        if selected.iter().all(|(index, _)| *index != target) {
            return empty_spans();
        }

        let window = SourceWindow::new(selected);
        // The cache must hold two full parse windows at once: split view
        // interleaves an old- and a new-side sequence per row, and a cache
        // smaller than that lets each side's miss evict the spans the other
        // side just parsed, degrading every cell to a full context re-parse.
        if self.capacity != 0 {
            self.capacity = self.capacity.max(window.lines.len().saturating_mul(2));
        }
        self.stats.bytes = self.stats.bytes.saturating_add(window.source.len());
        let global = highlight_source(&mut self.highlighter, theme, language, &window.source);
        let per_line = global.as_deref().map_or_else(
            || vec![Vec::new(); window.lines.len()],
            |spans| window.split(spans),
        );
        let mut target_spans = None;
        for ((line, _, _, _), spans) in window.lines.iter().zip(per_line) {
            let spans: Arc<[HighlightSpan]> = Arc::from(spans);
            if *line == target {
                target_spans = Some(Arc::clone(&spans));
            }
            self.store(sequence_line_key(sequence_key, *line), spans);
        }
        if self.capacity == 0 {
            self.sequences.remove(&sequence_key);
        }
        target_spans.unwrap_or_else(empty_spans)
    }

    /// Highlights all supplied lines in one parse, preserving multiline state.
    #[must_use]
    pub fn highlight_sequential<'a, 'hint, T>(
        &mut self,
        theme: &SyntaxTheme,
        hint: impl Into<LanguageHint<'hint>>,
        lines: T,
    ) -> Vec<Vec<HighlightSpan>>
    where
        T: IntoIterator<Item = &'a str>,
    {
        let selected: Vec<(usize, &str)> = lines.into_iter().enumerate().collect();
        let window = SourceWindow::new(selected);
        let Some(language) = resolve_language(hint, &window.source) else {
            return vec![Vec::new(); window.lines.len()];
        };
        highlight_source(&mut self.highlighter, theme, language, &window.source)
            .as_deref()
            .map_or_else(
                || vec![Vec::new(); window.lines.len()],
                |spans| window.split(spans),
            )
    }

    /// Appends lines and returns spans in the same order as the appended lines.
    /// Only bounded preceding context is retained between calls.
    pub fn append_lines<'a>(
        &mut self,
        theme: &SyntaxTheme,
        sequence: &mut SyntaxSequence,
        lines: impl IntoIterator<Item = &'a str>,
    ) -> Vec<Vec<HighlightSpan>> {
        let appended = lines.into_iter().map(str::to_owned).collect::<Vec<_>>();
        if appended.is_empty() {
            return Vec::new();
        }
        let previous_len = sequence.lines.len();
        sequence.lines.extend(appended);
        let first = previous_len.saturating_sub(PARSE_CONTEXT_LINES);
        let target_offset = previous_len - first;
        let mut highlighted = self.highlight_sequential(
            theme,
            sequence.hint.as_str(),
            sequence.lines[first..].iter().map(String::as_str),
        );
        let result = highlighted.split_off(target_offset);
        if sequence.lines.len() > PARSE_CONTEXT_LINES {
            let discard = sequence.lines.len() - PARSE_CONTEXT_LINES;
            sequence.lines.drain(..discard);
        }
        result
    }

    fn reserve_sequence(&mut self, key: Fingerprint) {
        if self.capacity == 0 || self.sequences.contains(&key) {
            return;
        }
        while self.sequences.len() >= MAX_SEQUENCES {
            let Some(evicted) = self.sequence_order.pop_front() else {
                break;
            };
            self.sequences.remove(&evicted);
        }
        self.sequences.insert(key);
        self.sequence_order.push_back(key);
    }

    fn store(&mut self, key: Fingerprint, spans: Arc<[HighlightSpan]>) {
        if self.capacity == 0 {
            return;
        }
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

fn sequence_line_key(sequence: Fingerprint, line: usize) -> Fingerprint {
    Fingerprint::of([
        sequence.as_bytes().as_slice(),
        &u64::try_from(line).unwrap_or(u64::MAX).to_le_bytes(),
    ])
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
        let spans = highlighter.highlight(&theme, "rs", source);
        assert!(!spans.is_empty());
        assert!(
            spans
                .iter()
                .all(|span| source.is_char_boundary(span.range.start)
                    && source.is_char_boundary(span.range.end))
        );
        let _ = highlighter.highlight(&theme, "RUST", source);
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
            let spans = highlighter.highlight(&theme, language, source);
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
                .highlight(&theme, "binary.zzz", "abc")
                .is_empty()
        );
        assert!(
            highlighter
                .highlight(&theme, "binary.zzz", "abc")
                .is_empty()
        );
        assert_eq!(highlighter.stats().hits, 1);
        assert_eq!(highlighter.stats().bytes, 0);
    }

    #[test]
    fn fifo_zero_capacity_and_theme_revision_behave_as_before() {
        let theme = DiffTheme::default();
        let mut fifo = SyntaxHighlighter::new(1);
        fifo.highlight(&theme, "rust", "fn a() {}");
        fifo.highlight(&theme, "rust", "fn b() {}");
        assert_eq!(fifo.stats().evictions, 1);
        let mut zero = SyntaxHighlighter::new(0);
        zero.highlight(&theme, "rust", "fn a() {}");
        zero.highlight(&theme, "rust", "fn a() {}");
        assert_eq!(zero.stats().misses, 2);
        let mut themes = SyntaxHighlighter::new(8);
        themes.highlight(&theme, "rust", "fn main() {}");
        themes.highlight(&DiffTheme::ayu().unwrap(), "rust", "fn main() {}");
        assert_eq!(themes.stats().misses, 2);
    }

    #[test]
    fn sequence_work_is_context_bounded_and_prefetches() {
        let owned: Vec<String> = (0..3_000)
            .map(|i| format!("let value_{i} = {i};"))
            .collect();
        let lines: Vec<&str> = owned.iter().map(String::as_str).collect();
        let sequence = Fingerprint::of([b"sequence".as_slice()]);
        let theme = DiffTheme::default();
        let mut highlighter = SyntaxHighlighter::new(8);
        highlighter.reserve_for_viewport(20);
        highlighter.highlight_in_sequence(
            &theme,
            "src/lib.rs",
            sequence,
            2_500,
            lines.iter().copied(),
        );
        let bytes = highlighter.stats().bytes;
        let full: usize = lines.iter().map(|line| line.len() + 1).sum();
        assert!(bytes < full / 2, "parsed {bytes} of {full} bytes");
        highlighter.highlight_in_sequence(
            &theme,
            "src/lib.rs",
            sequence,
            2_510,
            lines.iter().copied(),
        );
        assert_eq!(
            highlighter.stats().hits,
            1,
            "prefetched viewport line should hit"
        );
    }

    #[test]
    fn split_view_sequences_share_the_cache_without_evicting_each_other() {
        let owned: Vec<String> = (0..1_100).map(|i| format!("let v{i} = {i};")).collect();
        let lines: Vec<&str> = owned.iter().map(String::as_str).collect();
        let theme = DiffTheme::default();
        let mut highlighter = SyntaxHighlighter::default();
        highlighter.reserve_for_viewport(5);
        let sides = [Fingerprint::of(["old"]), Fingerprint::of(["new"])];
        for _frame in 0..3 {
            for row in 1_030..1_035 {
                for sequence in sides {
                    let spans = highlighter.highlight_in_sequence(
                        &theme,
                        "src/lib.rs",
                        sequence,
                        row,
                        lines.iter().copied(),
                    );
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
        // Hosts may render before ever calling `reserve_for_viewport`; with no
        // prefetch each row still misses once, but repeated frames over the
        // same viewport must not parse again.
        let owned: Vec<String> = (0..1_100).map(|i| format!("let v{i} = {i};")).collect();
        let lines: Vec<&str> = owned.iter().map(String::as_str).collect();
        let theme = DiffTheme::default();
        let mut highlighter = SyntaxHighlighter::default();
        let sides = [Fingerprint::of(["old"]), Fingerprint::of(["new"])];
        for _frame in 0..3 {
            for row in 1_030..1_035 {
                for sequence in sides {
                    let _ = highlighter.highlight_in_sequence(
                        &theme,
                        "src/lib.rs",
                        sequence,
                        row,
                        lines.iter().copied(),
                    );
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
        let sequence = Fingerprint::of(["multiline-comment"]);
        highlighter.reserve_for_viewport(lines.len());
        for (index, line) in lines.iter().enumerate() {
            let spans = highlighter.highlight_in_sequence(
                &theme,
                "rs",
                sequence,
                index,
                lines.iter().copied(),
            );
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
        let rendered = highlighter.highlight_sequential(&theme, "rust", lines);
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
        let html_spans = highlighter.highlight(&theme, "html", html);
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
        let markdown_spans = highlighter.highlight(&theme, "markdown", markdown);
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
