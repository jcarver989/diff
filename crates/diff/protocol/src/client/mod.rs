mod document_replica;
mod message;
mod transport;

pub use document_replica::DocumentReplica;
pub use message::ClientCommand;
pub use transport::{LocalClientMessageTransport, LocalClientTransport};

use clankerdiff_core::{DiffDocument, ReviewCapabilities};

#[must_use]
pub fn capabilities(connected: bool, document: Option<&DiffDocument>) -> ReviewCapabilities {
    let working_tree = document.is_none_or(|document| document.target.scope().is_some());
    ReviewCapabilities {
        repository: connected && working_tree,
        refresh: connected,
        scope: connected,
        submit: document.is_some(),
        lookups: connected,
        ..ReviewCapabilities::default()
    }
}
