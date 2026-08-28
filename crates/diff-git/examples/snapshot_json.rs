//! Writes a repository diff snapshot as `DiffDocument` JSON.

use diff_core::DiffScope;
use diff_git::GitRepository;
use std::{env, path::PathBuf, process::ExitCode};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<String, Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let repository = PathBuf::from(arguments.next().unwrap_or_else(|| ".".into()));
    let requested_scope = arguments.next().and_then(|scope| scope.into_string().ok());
    let scope = match requested_scope.as_deref() {
        None | Some("unstaged") => DiffScope::Unstaged,
        Some("staged") => DiffScope::Staged,
        Some("both") => DiffScope::Both,
        Some(scope) => {
            return Err(format!("unknown scope `{scope}`; use unstaged, staged, or both").into());
        }
    };
    if arguments.next().is_some() {
        return Err("usage: snapshot_json [REPOSITORY] [unstaged|staged|both]".into());
    }

    let repository = GitRepository::discover(repository).await?;
    let document = repository.snapshot(scope).await?;
    serde_json::to_string(&document).map_err(Into::into)
}
