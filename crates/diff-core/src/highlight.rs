//! Pure-Rust syntax highlighting with UTF-8 byte spans and a bounded cache.

use crate::{DiffTheme, Fingerprint, FontStyle, HighlightSpan, Rgba};
use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    fmt,
    sync::OnceLock,
};
use syntect::{
    easy::HighlightLines,
    highlighting::{
        FontStyle as SyntectFontStyle, HighlightIterator, HighlightState, Highlighter, Style,
    },
    parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet},
};

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
    cache: HashMap<Fingerprint, Vec<HighlightSpan>>,
    order: VecDeque<Fingerprint>,
    sequences: HashMap<Fingerprint, SequenceState>,
    sequence_order: VecDeque<Fingerprint>,
    stats: HighlightStats,
}

struct SequenceState {
    parse: ParseState,
    highlight: HighlightState,
    next_line: usize,
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
        Self::new(128)
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
    ) -> Vec<HighlightSpan> {
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
            return spans.clone();
        }
        self.stats.misses += 1;
        self.stats.bytes += source.len();
        let spans = self.highlight_uncached(theme, &language, source);
        self.store(key, &spans);
        spans
    }

    pub fn highlight_in_sequence<'a, T>(
        &mut self,
        theme: &DiffTheme,
        language: &str,
        sequence: Fingerprint,
        target: usize,
        lines: T,
    ) -> Vec<HighlightSpan>
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
        let line_key = Fingerprint::of([
            sequence_key.as_bytes().as_slice(),
            &u64::try_from(target).unwrap_or(u64::MAX).to_le_bytes(),
        ]);
        if let Some(spans) = self.cache.get(&line_key) {
            self.stats.hits += 1;
            return spans.clone();
        }
        self.stats.misses += 1;

        let syntax = resolve_syntax(self.syntax, &language);
        if !self.sequences.contains_key(&sequence_key) && self.capacity != 0 {
            if self.sequences.len() >= self.capacity
                && let Some(evicted) = self.sequence_order.pop_front()
            {
                self.sequences.remove(&evicted);
            }
            self.sequence_order.push_back(sequence_key);
        }
        let state = self.sequences.entry(sequence_key).or_insert_with(|| {
            let highlighter = Highlighter::new(theme.syntax());
            SequenceState {
                parse: ParseState::new(syntax),
                highlight: HighlightState::new(&highlighter, ScopeStack::new()),
                next_line: 0,
            }
        });
        if state.next_line > target {
            let highlighter = Highlighter::new(theme.syntax());
            *state = SequenceState {
                parse: ParseState::new(syntax),
                highlight: HighlightState::new(&highlighter, ScopeStack::new()),
                next_line: 0,
            };
        }

        let highlighter = Highlighter::new(theme.syntax());
        let mut target_spans = Vec::new();
        let mut parsed = Vec::new();
        for (index, line) in lines.into_iter().enumerate().skip(state.next_line) {
            if index > target {
                break;
            }
            self.stats.bytes += line.len();
            let spans = spans_for_state(
                &mut state.parse,
                &mut state.highlight,
                &highlighter,
                self.syntax,
                line,
            );
            state.next_line = index.saturating_add(1);
            let key = Fingerprint::of([
                sequence_key.as_bytes().as_slice(),
                &u64::try_from(index).unwrap_or(u64::MAX).to_le_bytes(),
            ]);
            if index == target {
                target_spans.clone_from(&spans);
            }
            parsed.push((key, spans));
        }
        for (key, spans) in parsed {
            self.store(key, &spans);
        }
        if self.capacity == 0 {
            self.sequences.remove(&sequence_key);
        }
        target_spans
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

    fn store(&mut self, key: Fingerprint, spans: &[HighlightSpan]) {
        if self.capacity == 0 {
            return;
        }
        if self.cache.len() >= self.capacity
            && let Some(evicted) = self.order.pop_front()
        {
            self.cache.remove(&evicted);
            self.stats.evictions += 1;
        }
        self.order.push_back(key);
        self.cache.insert(key, spans.to_vec());
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
    fn multiline_is_sequential() {
        let theme = DiffTheme::default();
        let highlighter = SyntaxHighlighter::new(0);
        let spans = highlighter.highlight_sequential(&theme, "rust", ["/*", " comment */"]);
        assert_eq!(spans.len(), 2);
    }
}
