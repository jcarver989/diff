//! Pure-Rust syntax highlighting with UTF-8 byte spans and a bounded cache.

use crate::theme::{DiffTheme, FontStyle, HighlightSpan, Rgba};
use std::collections::{HashMap, VecDeque};
use syntect::{easy::HighlightLines, highlighting::Style, parsing::SyntaxSet};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    theme_revision: [u8; 32],
    language: String,
    source_hash: [u8; 32],
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

/// A reusable syntax highlighter. Entries are evicted oldest-first when full.
pub struct SyntaxHighlighter {
    syntax: SyntaxSet,
    capacity: usize,
    cache: HashMap<CacheKey, Vec<HighlightSpan>>,
    order: VecDeque<CacheKey>,
    stats: HighlightStats,
}
impl std::fmt::Debug for SyntaxHighlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntaxHighlighter")
            .field("capacity", &self.capacity)
            .field("entries", &self.cache.len())
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
    pub fn new(capacity: usize) -> Self {
        Self {
            syntax: two_face::syntax::extra_newlines(),
            capacity,
            cache: HashMap::new(),
            order: VecDeque::new(),
            stats: HighlightStats::default(),
        }
    }
    pub fn with_capacity(capacity: usize) -> Self {
        Self::new(capacity)
    }
    pub fn stats(&self) -> HighlightStats {
        self.stats
    }
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.order.clear();
    }
    pub fn reset_stats(&mut self) {
        self.stats = HighlightStats::default();
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
        let key = CacheKey {
            theme_revision: theme.revision(),
            language: language.clone(),
            source_hash: *blake3::hash(source.as_bytes()).as_bytes(),
        };
        if let Some(value) = self.cache.get(&key) {
            self.stats.hits += 1;
            return value.clone();
        }
        self.stats.misses += 1;
        let spans = self.highlight_uncached(theme, &language, source);
        self.stats.bytes += source.len();
        if self.capacity != 0 {
            if self.cache.len() >= self.capacity
                && let Some(old) = self.order.pop_front()
            {
                self.cache.remove(&old);
                self.stats.evictions += 1;
            }
            self.order.push_back(key.clone());
            self.cache.insert(key, spans.clone());
        }
        spans
    }
    /// Alias with a name useful at call sites that make caching explicit.
    pub fn highlight_cached(
        &mut self,
        theme: &DiffTheme,
        language: &str,
        source: &str,
    ) -> Vec<HighlightSpan> {
        self.highlight(theme, language, source)
    }

    /// Highlights lines in order with one parser, preserving multiline state.
    pub fn highlight_sequential<'a, I>(
        &self,
        theme: &DiffTheme,
        language: &str,
        lines: I,
    ) -> Vec<Vec<HighlightSpan>>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let syntax = resolve_syntax(&self.syntax, language);
        let mut highlighter = HighlightLines::new(syntax, &theme.syntax);
        lines
            .into_iter()
            .map(|line| spans_for_line(&mut highlighter, &self.syntax, line))
            .collect()
    }
    fn highlight_uncached(
        &self,
        theme: &DiffTheme,
        language: &str,
        source: &str,
    ) -> Vec<HighlightSpan> {
        let mut offset = 0;
        let mut output = Vec::new();
        for (line, spans) in source.split_inclusive('\n').zip(self.highlight_sequential(
            theme,
            language,
            source.split_inclusive('\n'),
        )) {
            for mut span in spans {
                span.range.start += offset;
                span.range.end += offset;
                output.push(span);
            }
            offset += line.len();
        }
        output
    }
}

fn canonical_language(language: &str) -> String {
    match language.trim().to_ascii_lowercase().as_str() {
        "rs" => "rust".into(),
        "js" => "javascript".into(),
        "ts" => "typescript".into(),
        "py" => "python".into(),
        "md" => "markdown".into(),
        "sh" | "shell" => "bash".into(),
        other => other.into(),
    }
}

fn resolve_syntax<'a>(set: &'a SyntaxSet, language: &str) -> &'a syntect::parsing::SyntaxReference {
    let aliases = canonical_language(language);
    set.find_syntax_by_token(&aliases)
        .or_else(|| set.find_syntax_by_extension(&aliases))
        .or_else(|| set.find_syntax_by_name(&aliases))
        .unwrap_or_else(|| set.find_syntax_plain_text())
}
fn spans_for_line(
    highlighter: &mut HighlightLines<'_>,
    set: &SyntaxSet,
    line: &str,
) -> Vec<HighlightSpan> {
    let Ok(parts) = highlighter.highlight_line(line, set) else {
        return vec![HighlightSpan {
            range: 0..line.len(),
            foreground: Rgba::default(),
            font_style: FontStyle::empty(),
        }];
    };
    let mut offset = 0;
    let mut result = Vec::with_capacity(parts.len());
    for (style, text) in parts {
        let end = offset + text.len();
        if end > offset {
            result.push(HighlightSpan {
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
    result
}
fn font_style(style: Style) -> FontStyle {
    let mut f = FontStyle::empty();
    if style
        .font_style
        .contains(syntect::highlighting::FontStyle::BOLD)
    {
        f |= FontStyle::BOLD;
    }
    if style
        .font_style
        .contains(syntect::highlighting::FontStyle::ITALIC)
    {
        f |= FontStyle::ITALIC;
    }
    if style
        .font_style
        .contains(syntect::highlighting::FontStyle::UNDERLINE)
    {
        f |= FontStyle::UNDERLINE;
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_and_utf8_ranges() {
        let t = DiffTheme::default();
        let mut h = SyntaxHighlighter::new(2);
        let s = "let café = 1;\n";
        let spans = h.highlight(&t, "rs", s);
        assert!(
            spans
                .iter()
                .all(|x| s.is_char_boundary(x.range.start) && s.is_char_boundary(x.range.end))
        );
        assert_eq!(h.stats().misses, 1);
        let _ = h.highlight(&t, "rust", s);
        assert_eq!(h.stats().hits, 1);
    }
    #[test]
    fn fifo_eviction() {
        let t = DiffTheme::default();
        let mut h = SyntaxHighlighter::new(1);
        h.highlight(&t, "text", "a");
        h.highlight(&t, "text", "b");
        assert_eq!(h.stats().evictions, 1);
    }
    #[test]
    fn multiline_is_sequential() {
        let t = DiffTheme::default();
        let h = SyntaxHighlighter::new(0);
        let out = h.highlight_sequential(&t, "rust", ["/*", " comment */"]);
        assert_eq!(out.len(), 2);
    }
}
