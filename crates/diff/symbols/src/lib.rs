//! Symbol definitions, references and uses across the files in a diff.
mod index;

pub use clankerdiff_syntax::{SymbolKind, SymbolReference};
pub use index::{Definition, SymbolCache, SymbolChange, SymbolIndex, SymbolUse};
