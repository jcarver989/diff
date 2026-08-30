//! Portable syntax highlighting APIs.
mod highlight;
mod language;

pub use diff_theme::{
    DiffPalette, DiffTheme, FontStyle, HighlightSpan, Rgba, SyntaxStyle, SyntaxTheme,
};
pub use highlight::{
    HighlightStats, PARSE_CONTEXT_LINES, SyntaxHighlighter, SyntaxSequence, empty_spans,
};
pub use language::{LanguageHint, resolve_language};
