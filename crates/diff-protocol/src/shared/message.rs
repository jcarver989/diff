use super::ProtocolError;
use serde::{Serialize, de::DeserializeOwned};

pub const LIVE_PROTOCOL_VERSION: u32 = 1;
pub const MAX_MESSAGE_BYTES: usize = 128 * 1024 * 1024;

pub(crate) fn encode_json<T: Serialize>(value: &T) -> Result<String, ProtocolError> {
    Ok(serde_json::to_string(value)?)
}

pub(crate) fn decode_json<T: DeserializeOwned>(text: &str) -> Result<T, ProtocolError> {
    Ok(serde_json::from_str(text)?)
}
