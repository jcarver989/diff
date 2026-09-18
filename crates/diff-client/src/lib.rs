mod client;
mod error;
mod header;
mod platform;
pub mod protocol;
mod state;
pub mod transport;

pub use clankerdiff_core::{DiffReviewEvent, DiffScope, RepositoryAction, ReviewCapabilities};
pub use client::{ClientSubscription, DiffClient};
pub use error::ClientError;
pub use header::{ConnectionHeader, ConnectionHeaderError};
pub use protocol::shared::{DiffSnapshot, RemoteError, RemoteErrorCode};
pub use state::{ClientOptions, ClientState, ConnectionState, ReconnectPolicy};
