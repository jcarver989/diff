//! Renderer-independent diff parsing, presentation, review, themes, and highlighting.

pub mod anchor;
pub mod error;
pub mod highlight;
pub mod model;
pub mod parser;
pub mod presentation;
pub mod review;
pub mod theme;

pub use anchor::*;
pub use error::*;
pub use highlight::*;
pub use model::*;
pub use parser::*;
pub use presentation::*;
pub use review::*;
pub use theme::*;
