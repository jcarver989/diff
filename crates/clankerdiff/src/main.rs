mod args;
mod preferences;
mod tui;
mod web;

use args::{Cli, Command, MarkdownArgs, OutputFormat, ReviewArgs, Ui};
use clap::Parser;
use diff_core::{
    MarkdownDocument, MarkdownReviewDecision, MarkdownReviewSubmission, ReviewSubmission,
};
use diff_git::GitRepository;
use serde_json::json;
use std::{
    fs,
    io::{self, Read},
    path::Path,
    process::ExitCode,
    sync::Arc,
};
use thiserror::Error;

#[tokio::main]
async fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Review(args) => match run(args).await {
            Ok(ReviewOutcome::Submitted) => ExitCode::SUCCESS,
            Ok(ReviewOutcome::Cancelled) => ExitCode::from(2),
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        },
        Command::Markdown(args) => match run_markdown(&args) {
            Ok(ReviewOutcome::Submitted) => ExitCode::SUCCESS,
            Ok(ReviewOutcome::Cancelled) => ExitCode::from(2),
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        },
        Command::Attach(args) => match tui::attach(&args.session_url) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        },
    }
}

async fn run(args: ReviewArgs) -> Result<ReviewOutcome, AppError> {
    let repository = GitRepository::discover(&args.repository).await?;
    let root = repository.root().to_path_buf();
    let submission = match args.ui {
        Ui::Desktop => diff_gpui_desktop::run_review(root.clone(), args.scope),
        Ui::Tui => {
            let document = repository.snapshot(args.scope).await?;
            web::run_tui_session(
                &repository,
                &document,
                args.scope,
                args.port,
                |session_url| tui::launch(session_url).map_err(|error| error.to_string()),
            )?
        }
        Ui::Web => {
            let document = repository.snapshot(args.scope).await?;
            web::run(
                &repository,
                &document,
                args.scope,
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

fn run_markdown(args: &MarkdownArgs) -> Result<ReviewOutcome, AppError> {
    let (source, source_path, file_name) = if args.path == "-" {
        if args.ui == Ui::Tui {
            return Err(AppError::MarkdownStdinTui);
        }
        let mut bytes = Vec::new();
        io::stdin().lock().read_to_end(&mut bytes)?;
        (decode_markdown(bytes)?, None, None)
    } else {
        let path = Path::new(&args.path);
        if path.is_dir() {
            return Err(AppError::MarkdownDirectory(args.path.clone()));
        }
        let bytes = fs::read(path)?;
        let source_path = Some(args.path.clone());
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        (decode_markdown(bytes)?, source_path, file_name)
    };
    let provisional =
        MarkdownDocument::parse_with_metadata(source_path.clone(), None, source.clone());
    let title = args.title.clone().or_else(|| {
        provisional
            .outline()
            .iter()
            .find(|heading| heading.level == 1)
            .map(|heading| heading.title.clone())
            .or(file_name)
    });
    let document = MarkdownDocument::parse_with_metadata(source_path.clone(), title, source);
    let submission = match args.ui {
        Ui::Tui => tui::run_markdown(Arc::new(document))?,
        Ui::Desktop => diff_gpui_desktop::run_markdown_review(document),
        Ui::Web => web::run_markdown(
            &document,
            &web::Options {
                port: 0,
                no_open: false,
                assets: None,
            },
        )?,
    };
    let Some(submission) = submission else {
        return Ok(ReviewOutcome::Cancelled);
    };
    write_markdown_submission(&submission, args, source_path.as_deref())?;
    Ok(ReviewOutcome::Submitted)
}

fn decode_markdown(bytes: Vec<u8>) -> Result<String, AppError> {
    String::from_utf8(bytes).map_err(|error| AppError::InvalidMarkdownUtf8(error.utf8_error()))
}

fn write_markdown_submission(
    submission: &MarkdownReviewSubmission,
    args: &MarkdownArgs,
    source_path: Option<&str>,
) -> Result<(), AppError> {
    match args.format {
        OutputFormat::Text => println!("{}", markdown_text_feedback(submission)),
        OutputFormat::Json => {
            let outcome = match submission.decision {
                MarkdownReviewDecision::Approved => "approved",
                MarkdownReviewDecision::ChangesRequested => "changes_requested",
            };
            let output = json!({
                "schema_version": 1,
                "document_kind": "markdown",
                "outcome": outcome,
                "source_path": source_path,
                "submission": submission,
            });
            serde_json::to_writer_pretty(io::stdout().lock(), &output)?;
            println!();
        }
    }
    Ok(())
}

fn markdown_text_feedback(submission: &MarkdownReviewSubmission) -> &str {
    match submission.decision {
        MarkdownReviewDecision::Approved => {
            "I reviewed the Markdown document and approve it with no comments."
        }
        MarkdownReviewDecision::ChangesRequested => &submission.formatted,
    }
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
    #[error("Markdown input path is a directory: {0}")]
    MarkdownDirectory(String),
    #[error("Markdown input is not valid UTF-8: {0}")]
    InvalidMarkdownUtf8(std::str::Utf8Error),
    #[error("Markdown from stdin cannot use the TUI; choose --ui desktop or --ui web")]
    MarkdownStdinTui,
    #[error("could not read Markdown input: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Tui(#[from] tui::TuiError),
    #[error(transparent)]
    Web(#[from] web::WebError),
    #[error("could not serialize review output: {0}")]
    Serialize(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use crate::{markdown_text_feedback, text_feedback};
    use diff_core::{MarkdownReview, MarkdownReviewDecision, Review};

    #[test]
    fn markdown_decisions_have_explicit_text() {
        let approved = MarkdownReview::default()
            .try_submission(MarkdownReviewDecision::Approved)
            .unwrap();
        assert!(markdown_text_feedback(&approved).contains("approve"));
        let changes = MarkdownReview::default()
            .try_submission(MarkdownReviewDecision::ChangesRequested)
            .unwrap();
        assert_eq!(
            markdown_text_feedback(&changes),
            "Changes requested, but no inline comments were provided."
        );
    }

    #[test]
    fn empty_review_is_an_approval() {
        let submission = Review::default().submission();
        assert_eq!(
            text_feedback(&submission),
            "I reviewed the working tree diff and approve it with no comments."
        );
    }
}
