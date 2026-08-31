//! Internal Git subprocess helpers.

use crate::GitError;
use std::{
    ffi::OsStr,
    path::Path,
    process::{Output, Stdio},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

pub(crate) async fn run<I, S>(
    cwd: &Path,
    operation: &'static str,
    args: I,
) -> Result<Output, GitError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|source| GitError::Spawn { operation, source })?;
    if output.status.success() {
        return Ok(output);
    }
    Err(GitError::CommandFailed {
        operation,
        status: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

pub(crate) struct CatFileBatch {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl CatFileBatch {
    pub(crate) fn start(cwd: &Path) -> Result<Self, GitError> {
        const OPERATION: &str = "read source blobs";
        let mut child = Command::new("git")
            .args(["cat-file", "--batch-command"])
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|source| GitError::Spawn {
                operation: OPERATION,
                source,
            })?;
        let stdin = child.stdin.take().ok_or_else(|| GitError::Spawn {
            operation: OPERATION,
            source: std::io::Error::other("Git batch stdin was not piped"),
        })?;
        let stdout = child.stdout.take().ok_or_else(|| GitError::Spawn {
            operation: OPERATION,
            source: std::io::Error::other("Git batch stdout was not piped"),
        })?;
        Ok(Self {
            _child: child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    pub(crate) async fn read_blob(
        &mut self,
        oid: &str,
        maximum: u64,
    ) -> Result<Result<Vec<u8>, u64>, GitError> {
        const OPERATION: &str = "read source blobs";
        let size = self.request_header("info", oid).await?;
        if size > maximum {
            return Ok(Err(size));
        }

        let contents_size = self.request_header("contents", oid).await?;
        if contents_size != size {
            return Err(invalid_batch_response("Git batch changed the blob size"));
        }
        let length = usize::try_from(size)
            .map_err(|_| invalid_batch_response("Git batch blob size does not fit in memory"))?;
        let mut bytes = vec![0; length];
        self.stdout
            .read_exact(&mut bytes)
            .await
            .map_err(|source| GitError::Spawn {
                operation: OPERATION,
                source,
            })?;
        let mut delimiter = [0_u8; 1];
        self.stdout
            .read_exact(&mut delimiter)
            .await
            .map_err(|source| GitError::Spawn {
                operation: OPERATION,
                source,
            })?;
        if delimiter != *b"\n" {
            return Err(invalid_batch_response(
                "Git batch omitted the blob delimiter",
            ));
        }
        Ok(Ok(bytes))
    }

    async fn request_header(&mut self, command: &str, oid: &str) -> Result<u64, GitError> {
        const OPERATION: &str = "read source blobs";
        self.stdin
            .write_all(format!("{command} {oid}\n").as_bytes())
            .await
            .map_err(|source| GitError::Spawn {
                operation: OPERATION,
                source,
            })?;
        self.stdin.flush().await.map_err(|source| GitError::Spawn {
            operation: OPERATION,
            source,
        })?;

        let mut header = String::new();
        self.stdout
            .read_line(&mut header)
            .await
            .map_err(|source| GitError::Spawn {
                operation: OPERATION,
                source,
            })?;
        let mut fields = header.split_ascii_whitespace();
        let (_resolved_oid, object_type, size, trailing) = (
            fields.next(),
            fields.next(),
            fields.next().and_then(|size| size.parse::<u64>().ok()),
            fields.next(),
        );
        match (object_type, size, trailing) {
            (Some("blob"), Some(size), None) => Ok(size),
            _ => Err(invalid_batch_response(
                "Git batch returned an invalid blob header",
            )),
        }
    }
}

fn invalid_batch_response(message: &str) -> GitError {
    GitError::CommandFailed {
        operation: "read source blobs",
        status: None,
        stderr: message.to_owned(),
    }
}
