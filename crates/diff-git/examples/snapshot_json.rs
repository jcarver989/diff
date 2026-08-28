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
    let scope = match arguments.next() {
        Some(value) => value.to_string_lossy().parse()?,
        None => DiffScope::Unstaged,
    };
    if arguments.next().is_some() {
        return Err("usage: snapshot_json [REPOSITORY] [unstaged|staged|both]".into());
    }

    let repository = GitRepository::discover(repository).await?;
    let document = repository.snapshot(scope).await?;
    serde_json::to_string(&document).map_err(Into::into)
}
