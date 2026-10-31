use super::{MAX_MESSAGE_BYTES, ProtocolError};
use std::io;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

pub struct StreamTransport<T, U> {
    reader: BufReader<T>,
    writer: U,
}

#[derive(Debug, Error)]
pub enum StreamError {
    #[error("stream closed")]
    Closed,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
}

impl<T: AsyncRead + Unpin, U: AsyncWrite + Unpin> StreamTransport<T, U> {
    pub fn new(reader: T, writer: U) -> Self {
        Self { reader: BufReader::new(reader), writer }
    }

    pub async fn read_line(&mut self) -> Result<String, StreamError> {
        let mut line = String::new();
        let read = (&mut self.reader).take(MAX_LINE_BYTES).read_line(&mut line).await?;
        if read == 0 {
            return Err(StreamError::Closed);
        }
        if !line.ends_with('\n') && read as u64 == MAX_LINE_BYTES {
            return Err(ProtocolError::TooLarge.into());
        }
        Ok(line)
    }

    pub async fn write_line(&mut self, line: &str) -> io::Result<()> {
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await
    }

    pub async fn shutdown(&mut self) {
        let _ = self.writer.shutdown().await;
    }
}

const MAX_LINE_BYTES: u64 = MAX_MESSAGE_BYTES as u64 + 1;
