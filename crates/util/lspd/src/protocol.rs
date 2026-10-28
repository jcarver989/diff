use lsp_types::Uri;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::bytes::{Bytes, BytesMut};
use tokio_util::codec::{Decoder, Encoder, LengthDelimitedCodec};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum DaemonRequest {
    Initialize { root: PathBuf, server: String },
    Call { id: i64, call: DaemonCall },
    Cancel { id: i64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum DaemonCall {
    Lsp { method: String, params: Value },
    Diagnostics { uri: Option<Uri> },
    QueueRefresh { uri: Uri },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum DaemonResponse {
    Initialized(Result<(), String>),
    Reply { id: i64, result: Result<Value, CallError> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum CallError {
    Lsp(LspErrorResponse),
    TimedOut { after: Duration },
    Closed,
    Daemon(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LspErrorResponse {
    pub(crate) code: i32,
    pub(crate) message: String,
}

pub(crate) const PROTOCOL_VERSION: u32 = 5;
pub(crate) const LSP_CONTENT_MODIFIED: i32 = -32801;
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

pub(crate) struct JsonFrames<T>(LengthDelimitedCodec, PhantomData<fn() -> T>);

impl<T> Default for JsonFrames<T> {
    fn default() -> Self {
        let codec = LengthDelimitedCodec::builder()
            .big_endian()
            .length_field_type::<u32>()
            .max_frame_length(MAX_MESSAGE_SIZE)
            .new_codec();
        Self(codec, PhantomData)
    }
}

impl<T: DeserializeOwned> Decoder for JsonFrames<T> {
    type Item = T;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<T>> {
        self.0.decode(src)?.map(|frame| serde_json::from_slice(&frame).map_err(io::Error::other)).transpose()
    }
}

impl<T: Serialize> Encoder<T> for JsonFrames<T> {
    type Error = io::Error;

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> io::Result<()> {
        let json = serde_json::to_vec(&item).map_err(io::Error::other)?;
        self.0.encode(Bytes::from(json), dst)
    }
}
