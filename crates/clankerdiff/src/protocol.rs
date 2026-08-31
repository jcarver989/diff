use diff_core::{DiffDocument, RepositoryAction, ReviewSubmission};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::io::{self, Read, Write};
use thiserror::Error;

const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq, Deserialize)]
pub enum SessionRequest {
    Document,
    RepositoryAction(RepositoryAction),
    Submit(ReviewSubmission),
    Cancel,
}

#[derive(Serialize)]
pub enum SessionRequestRef<'a> {
    Document,
    RepositoryAction(&'a RepositoryAction),
    Submit(&'a ReviewSubmission),
    Cancel,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
pub enum SessionResponse {
    Document(DiffDocument),
    Accepted,
    RepositoryError(String),
    ProtocolError(String),
}

#[derive(Serialize)]
pub enum SessionResponseRef<'a> {
    Document(&'a DiffDocument),
    Accepted,
    RepositoryError(&'a str),
    ProtocolError(&'a str),
}

pub fn read_request(stream: &mut impl Read) -> Result<SessionRequest, ProtocolError> {
    read_message(stream, MAX_REQUEST_BYTES)
}

pub fn write_request(
    stream: &mut impl Write,
    request: &SessionRequestRef<'_>,
) -> Result<(), ProtocolError> {
    write_message(stream, request, MAX_REQUEST_BYTES)
}

pub fn read_response(stream: &mut impl Read) -> Result<SessionResponse, ProtocolError> {
    read_message(stream, MAX_RESPONSE_BYTES)
}

pub fn write_response(
    stream: &mut impl Write,
    response: &SessionResponseRef<'_>,
) -> Result<(), ProtocolError> {
    write_message(stream, response, MAX_RESPONSE_BYTES)
}

fn read_message<T: DeserializeOwned>(
    stream: &mut impl Read,
    maximum_size: usize,
) -> Result<T, ProtocolError> {
    let mut length = [0_u8; size_of::<u32>()];
    stream.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > maximum_size {
        return Err(ProtocolError::MessageTooLarge {
            size: length,
            maximum_size,
        });
    }

    let mut body = vec![0; length];
    stream.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(ProtocolError::Json)
}

fn write_message<T: Serialize>(
    stream: &mut impl Write,
    message: &T,
    maximum_size: usize,
) -> Result<(), ProtocolError> {
    let body = serde_json::to_vec(message)?;
    if body.len() > maximum_size {
        return Err(ProtocolError::MessageTooLarge {
            size: body.len(),
            maximum_size,
        });
    }
    let length = u32::try_from(body.len()).map_err(|_| ProtocolError::MessageTooLarge {
        size: body.len(),
        maximum_size,
    })?;

    stream.write_all(&length.to_be_bytes())?;
    stream.write_all(&body)?;
    stream.flush().map_err(ProtocolError::Io)
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("session message I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("session message contains invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("session message is {size} bytes, exceeding the {maximum_size}-byte limit")]
    MessageTooLarge { size: usize, maximum_size: usize },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    #[test]
    fn exchanges_length_delimited_messages() {
        let (mut writer, mut reader) = UnixStream::pair().unwrap();
        write_request(
            &mut writer,
            &SessionRequestRef::RepositoryAction(&RepositoryAction::Refresh),
        )
        .unwrap();
        assert_eq!(
            read_request(&mut reader).unwrap(),
            SessionRequest::RepositoryAction(RepositoryAction::Refresh)
        );

        write_response(
            &mut reader,
            &SessionResponseRef::RepositoryError("repository changed"),
        )
        .unwrap();
        assert_eq!(
            read_response(&mut writer).unwrap(),
            SessionResponse::RepositoryError("repository changed".to_owned())
        );
    }

    #[test]
    fn rejects_an_oversized_announced_message() {
        let (mut writer, mut reader) = UnixStream::pair().unwrap();
        let oversized_length = u32::try_from(MAX_REQUEST_BYTES + 1).unwrap();
        writer.write_all(&oversized_length.to_be_bytes()).unwrap();
        assert!(matches!(
            read_request(&mut reader),
            Err(ProtocolError::MessageTooLarge { .. })
        ));
    }
}
