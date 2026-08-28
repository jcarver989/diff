//! Internal Git subprocess helpers.

use crate::GitError;
use std::{ffi::OsString, path::Path, process::Output};
use tokio::process::Command;

pub(crate) async fn run(
    cwd: &Path,
    operation: &'static str,
    args: impl IntoIterator<Item = OsString>,
) -> Result<Output, GitError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|source| GitError::Spawn { operation, source })?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(GitError::CommandFailed {
            operation,
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

pub(crate) fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}
