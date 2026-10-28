//! Symbol definitions, references and uses across the files in a diff.
mod index;
mod lookup;

pub use clankerdiff_syntax::{SymbolKind, SymbolReference};
pub use index::{Definition, SymbolCache, SymbolChange, SymbolIndex, SymbolUse};
pub use lookup::{Activation, CardContent, DefinitionCard, HINT_KEYS, Mark, NameHints, NameLookup, NamePick, Settled};
