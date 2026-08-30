//! Portable syntax highlighting APIs.
mod highlight;
mod language;

pub use diff_fingerprint::SourceSequenceId;
pub use diff_theme::{
    DiffPalette, DiffTheme, FontStyle, HighlightSpan, Rgba, SyntaxStyle, SyntaxTheme,
};
pub use highlight::{
    CacheConfig, CacheKey, HighlightStats, PARSE_CONTEXT_LINES, SequenceLine, SyntaxHighlighter,
    SyntaxStream, ThemedHighlighter, empty_spans,
};
pub use language::{LanguageHint, resolve_language};
