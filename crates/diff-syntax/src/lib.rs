//! Portable syntax highlighting APIs.
mod highlight;
mod language;

pub use clankerdiff_fingerprint::SourceSequenceId;
pub use clankerdiff_theme::{
    DiffPalette, DiffTheme, FontStyle, HighlightSpan, Rgba, SyntaxStyle, SyntaxTheme,
};
pub use highlight::{
    CacheConfig, CacheKey, DocumentHighlights, HighlightStats, SyntaxHighlighter, SyntaxStream,
    SyntaxStreamError, SyntaxStreamUpdate, ThemedHighlighter, empty_spans,
};
pub use language::{LanguageHint, resolve_language};
