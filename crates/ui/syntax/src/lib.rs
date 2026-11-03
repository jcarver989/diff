//! Portable syntax highlighting APIs.
mod cell;
mod error;
mod highlight;
mod incremental;
mod language;
mod spans;
mod symbols;

pub use cell::SyntaxCell;
pub use clankerdiff_fingerprint::Fingerprint;
pub use clankerdiff_ui_core::{FontStyle, HighlightId, HighlightSpan, Rgba, SyntaxStyle, SyntaxTheme};
pub use error::SyntaxError;
pub use highlight::{
    DocumentHighlights, MAX_SOURCE_BYTES, SourceSyntax, SyntaxStream, SyntaxStreamUpdate, analyze, empty_spans,
    thread_work,
};
pub use incremental::SyntaxWorkStats;
pub use language::{LanguageHint, resolve_language};
pub use symbols::{LineSpan, SymbolDefinition, SymbolKind, SymbolReference, SymbolTags, has_symbols};
