use http::{HeaderName, HeaderValue};
use std::{fmt, str::FromStr};
use thiserror::Error;

#[derive(Clone)]
pub struct ConnectionHeader {
    name: HeaderName,
    value: HeaderValue,
}

impl ConnectionHeader {
    pub fn name(&self) -> &HeaderName {
        &self.name
    }

    pub fn value(&self) -> &HeaderValue {
        &self.value
    }
}

impl fmt::Debug for ConnectionHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConnectionHeader")
            .field("name", &self.name)
            .field("value", &"<redacted>")
            .finish()
    }
}

impl FromStr for ConnectionHeader {
    type Err = ConnectionHeaderError;

    fn from_str(header: &str) -> Result<Self, Self::Err> {
        let (name, value) = header
            .split_once(':')
            .ok_or(ConnectionHeaderError::MissingSeparator)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(ConnectionHeaderError::EmptyName);
        }
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| ConnectionHeaderError::InvalidName)?;
        if is_reserved(&name) {
            return Err(ConnectionHeaderError::ReservedName(name.to_string()));
        }
        let mut value =
            HeaderValue::from_str(value.trim()).map_err(|_| ConnectionHeaderError::InvalidValue)?;
        value.set_sensitive(true);
        Ok(Self { name, value })
    }
}

#[derive(Debug, Error)]
pub enum ConnectionHeaderError {
    #[error("header must use NAME: VALUE syntax")]
    MissingSeparator,
    #[error("header name must not be empty")]
    EmptyName,
    #[error("invalid header name")]
    InvalidName,
    #[error("invalid value for header")]
    InvalidValue,
    #[error("header `{0}` is managed by the WebSocket client")]
    ReservedName(String),
}

fn is_reserved(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "host"
            | "connection"
            | "upgrade"
            | "sec-websocket-key"
            | "sec-websocket-version"
            | "sec-websocket-protocol"
            | "sec-websocket-extensions"
    )
}
