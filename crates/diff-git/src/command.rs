//! Internal Git subprocess helpers.

use crate::GitError;
use std::{ffi::OsStr, path::Path, process::Output};
use tokio::process::Command;

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
