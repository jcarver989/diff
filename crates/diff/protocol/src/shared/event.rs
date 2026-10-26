use super::{
    ProtocolError, RemoteError,
    message::{decode_json, encode_json},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::convert::Infallible;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event<T> {
    Initialize {
        protocol_version: u32,
        repository_root: String,
    },
    Document(T),
    RequestResult(Result<(), RemoteError>),
    Health {
        error: Option<RemoteError>,
    },
    Error(RemoteError),
}

impl<T> Event<T> {
    pub fn map<U>(self, document: impl FnOnce(T) -> U) -> Event<U> {
        let Ok(event) = self.try_map(|value| Ok::<_, Infallible>(document(value)));
        event
    }

    pub fn try_map<U, E>(self, document: impl FnOnce(T) -> Result<U, E>) -> Result<Event<U>, E> {
        Ok(match self {
            Self::Document(value) => Event::Document(document(value)?),
            Self::Initialize {
                protocol_version,
                repository_root,
            } => Event::Initialize {
                protocol_version,
                repository_root,
            },
            Self::RequestResult(result) => Event::RequestResult(result),
            Self::Health { error } => Event::Health { error },
            Self::Error(error) => Event::Error(error),
        })
    }
}

impl<T: Serialize> Event<T> {
    pub fn encode(&self) -> Result<String, ProtocolError> {
        encode_json(self)
    }
}

impl<T: DeserializeOwned> Event<T> {
    pub fn decode(text: &str) -> Result<Self, ProtocolError> {
        decode_json(text)
    }
}
