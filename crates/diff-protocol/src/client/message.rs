use crate::shared::{ProtocolError, decode_json, encode_json};
use clankerdiff_core::{DiffScope, RepositoryAction};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientCommand {
    Initialize {
        protocol_version: u32,
        scope: DiffScope,
    },
    SetScope(DiffScope),
    Apply(RepositoryAction),
    Refresh,
}

impl ClientCommand {
    pub fn encode(&self) -> Result<String, ProtocolError> {
        encode_json(self)
    }

    pub fn decode(text: &str) -> Result<Self, ProtocolError> {
        decode_json(text)
    }
}
