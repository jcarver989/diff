mod args;
mod tui;
mod web;

use args::{Cli, Command, OutputFormat, ReviewArgs, Ui};
use clap::Parser;
use diff_core::ReviewSubmission;
use diff_git::GitRepository;
use serde_json::json;
use std::{io, process::ExitCode, sync::Arc};
use thiserror::Error;

#[tokio::main]
async fn main() -> ExitCode {
    let Cli {
        command: Command::Review(args),
    } = Cli::parse();

    match run(args).await {
        Ok(ReviewOutcome::Submitted) => ExitCode::SUCCESS,
        Ok(ReviewOutcome::Cancelled) => ExitCode::from(2),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: ReviewArgs) -> Result<ReviewOutcome, AppError> {
    let repository = GitRepository::discover(&args.repository).await?;
    let root = repository.root().to_path_buf();
    let submission = match args.ui {
        Ui::Desktop => diff_gpui_desktop::run_review(root.clone(), args.scope),
        Ui::Tui => {
            let document = Arc::new(repository.snapshot(args.scope).await?);
            tui::run(document)?
        }
        Ui::Web => {
            let document = Arc::new(repository.snapshot(args.scope).await?);
            web::run(
                document.as_ref(),
                &web::Options {
                    port: args.port,
                    no_open: args.no_open,
                    assets: args.web_assets.clone(),
                },
            )?
        }
    };

    let Some(submission) = submission else {
        return Ok(ReviewOutcome::Cancelled);
    };
    write_submission(&submission, &args, &root)?;
    Ok(ReviewOutcome::Submitted)
}

fn write_submission(
    submission: &ReviewSubmission,
    args: &ReviewArgs,
    repository_root: &std::path::Path,
) -> Result<(), AppError> {
    match args.format {
        OutputFormat::Text => println!("{}", text_feedback(submission)),
        OutputFormat::Json => {
            let outcome = if submission.comments.is_empty() {
                "approved"
            } else {
                "changes_requested"
            };
            let output = json!({
                "schema_version": 1,
                "outcome": outcome,
                "repository_root": repository_root,
                "scope": args.scope.to_string(),
                "submission": submission,
            });
            serde_json::to_writer_pretty(io::stdout().lock(), &output)?;
            println!();
        }
    }
    Ok(())
}

fn text_feedback(submission: &ReviewSubmission) -> &str {
    if submission.comments.is_empty() {
        "I reviewed the working tree diff and approve it with no comments."
    } else {
        &submission.formatted
    }
}

enum ReviewOutcome {
    Submitted,
    Cancelled,
}

#[derive(Debug, Error)]
enum AppError {
    #[error(transparent)]
    Git(#[from] diff_git::GitError),
    #[error(transparent)]
    Tui(#[from] tui::TuiError),
    #[error(transparent)]
    Web(#[from] web::WebError),
    #[error("could not serialize review output: {0}")]
    Serialize(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use crate::text_feedback;
    use diff_core::Review;

    #[test]
    fn empty_review_is_an_approval() {
        let submission = Review::default().submission();
        assert_eq!(
            text_feedback(&submission),
            "I reviewed the working tree diff and approve it with no comments."
        );
    }
}
