//! Symbol definitions, references and uses across the files in a diff.
mod index;
mod lookup;
mod syntax;

pub use index::{Definition, SymbolChange, SymbolIndex, SymbolUse};
pub use lookup::{Activation, CardCode, CardContent, DefinitionCard, Hold, NameLookup, Settled};
pub use rouge_syntax::{SymbolKind, SymbolReference};
pub use syntax::{DocumentSyntax, HUNK_WINDOW_LINES};
