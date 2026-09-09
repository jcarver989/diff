//! Renderer-neutral Markdown documents, anchors, reviews, and append streams.
#[cfg(feature = "review")]
mod anchor;
#[cfg(feature = "review")]
pub mod commands;
mod document;
#[cfg(feature = "review")]
mod review;
#[cfg(feature = "review")]
mod session;
mod stream;

#[cfg(feature = "review")]
pub use anchor::*;
pub use clankerdiff_fingerprint::{Fingerprint, FingerprintError};
pub use document::*;
#[cfg(feature = "review")]
pub use commands::{MarkdownFocusPane, MarkdownReviewCommand};
#[cfg(feature = "review")]
pub use review::*;
#[cfg(feature = "review")]
pub use session::*;
pub use stream::{MarkdownStream, MarkdownStreamIdentity, MarkdownStreamUpdate};
