//! Portable syntax highlighting APIs.
mod error;
mod highlight;
mod incremental;
mod language;

pub use clankerdiff_fingerprint::Fingerprint;
pub use clankerdiff_theme::{FontStyle, HighlightSpan, Rgba, SyntaxStyle, SyntaxTheme};
pub use error::SyntaxError;
pub use highlight::{
    CacheConfig, CacheKey, CacheUsage, DocumentHighlights, HighlightStats, SyntaxHighlighter,
    SyntaxStream, SyntaxStreamUpdate, ThemedHighlighter, empty_spans,
};
pub use incremental::SyntaxWorkStats;
pub use language::{LanguageHint, resolve_language};
