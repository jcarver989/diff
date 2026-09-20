mod document_cache;
mod message;
mod transport;

pub use document_cache::DocumentCache;
pub use message::ClientCommand;
pub use transport::{LocalClientMessageTransport, LocalClientTransport};

use clankerdiff_core::ReviewCapabilities;

#[must_use]
pub fn capabilities(connected: bool, document: bool) -> ReviewCapabilities {
    ReviewCapabilities {
        repository: connected,
        refresh: connected,
        scope: connected,
        submit: document,
        ..ReviewCapabilities::default()
    }
}
