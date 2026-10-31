mod document;
mod error;
mod event;
mod message;
#[cfg(feature = "stream")]
mod stream;
mod transport;

pub use document::{DocumentUpdate, FileEntry};
pub use error::{ProtocolError, RemoteError, RemoteErrorCode};
pub use event::Event;
pub use message::{LIVE_PROTOCOL_VERSION, MAX_MESSAGE_BYTES};
#[cfg(feature = "stream")]
pub use stream::{StreamError, StreamTransport};
pub use transport::{LocalEnd, TransportClosed};

pub(crate) use message::{decode_json, encode_json};
