use crate::shared::{ProtocolError, decode_json, encode_json};
use clankerdiff_core::{RepositoryAction, ReviewSubmission, ReviewTarget};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientCommand {
    Initialize {
        protocol_version: u32,
        target: ReviewTarget,
    },
    SetTarget(ReviewTarget),
    Apply(RepositoryAction),
    Refresh,
    Submit(ReviewSubmission),
    Cancel,
}

impl ClientCommand {
    pub fn encode(&self) -> Result<String, ProtocolError> {
        encode_json(self)
    }

    pub fn decode(text: &str) -> Result<Self, ProtocolError> {
        decode_json(text)
    }
}
