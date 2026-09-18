mod args;
mod preferences;
mod protocol;
mod session;
mod tui;

use args::{
    CapabilitiesArgs, Cli, Command, ConnectArgs, MarkdownArgs, OutputFormat, ReviewArgs,
    TuiPlacement, Ui,
};
use clankerdiff_client::DiffClient;
use clankerdiff_core::{DiffScope, ReviewSubmission};
use clankerdiff_git::GitRepository;
use clankerdiff_markdown::{MarkdownDocument, MarkdownReviewDecision, MarkdownReviewSubmission};
use clankerdiff_protocol::{CapabilityResponse, PROTOCOL_VERSION, ReviewOutcome, ReviewResponse};
use clankerdiff_server::{DiffServer, ServerOptions};
use clap::Parser;
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
};
use thiserror::Error;

#[tokio::main]
async fn main() -> ExitCode {
    match dispatch(Cli::parse().command).await {
        Ok(exit) => exit,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn dispatch(command: Command) -> Result<ExitCode, AppError> {
    match command {
        Command::Serve(args) => {
            let server = DiffServer::open(args.repository, ServerOptions::default()).await?;
            let listener = server.listen(args.listen).await?;
            eprintln!(
                "Serving repository at ws://{}/ws (trusted network only; no authentication)",
                listener.local_addr()
            );
            tokio::signal::ctrl_c().await?;
            server.shutdown().await?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Connect(args) => {
            let format = args.format;
            let response = connect(args).await?;
            write_review_response(&response, format)?;
            Ok(exit_code(response.outcome()))
        }
        Command::Review(args) => {
            let format = args.format;
            let response = run(args).await?;
            write_review_response(&response, format)?;
            Ok(exit_code(response.outcome()))
        }
        Command::Markdown(args) => {
            let format = args.format;
            let response = run_markdown(&args)?;
            write_review_response(&response, format)?;
            Ok(exit_code(response.outcome()))
        }
        Command::Capabilities(args) => {
            write_capabilities(&args)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Attach(args) => {
            tui::attach(args.socket_path).await?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

async fn connect(args: ConnectArgs) -> Result<ReviewResponse, AppError> {
    #[cfg(not(feature = "desktop"))]
    if args.ui == Ui::Desktop {
        return Err(AppError::DesktopDisabled);
    }
    let client = DiffClient::connect(&args.url, args.scope.into()).await?;
    let outcome = match args.ui {
        Ui::Tui => tui::run_client(&client),
        #[cfg(feature = "desktop")]
        Ui::Desktop => Ok(clankerdiff_gpui_desktop::run_client_review(client.clone())),
        #[cfg(not(feature = "desktop"))]
        Ui::Desktop => return Err(AppError::DesktopDisabled),
    };
    let state = client.state();
    client.close().await?;
    let snapshot = state.snapshot.as_ref();
    Ok(diff_response(
        snapshot.map_or_else(PathBuf::new, |snapshot| {
            snapshot.document.repo_root.clone().into()
        }),
        snapshot.map_or(args.scope, |snapshot| snapshot.scope),
        outcome?,
    ))
}

async fn run(args: ReviewArgs) -> Result<ReviewResponse, AppError> {
    let repository = GitRepository::discover(&args.repository).await?;
    let root = repository.root().to_path_buf();
    let submission = match args.ui {
        #[cfg(not(feature = "desktop"))]
        Ui::Desktop => return Err(AppError::DesktopDisabled),
        #[cfg(feature = "desktop")]
        Ui::Desktop => {
            clankerdiff_gpui_desktop::run_review(clankerdiff_gpui_desktop::args::CliArgs {
                repository: root.clone(),
                scope: args.scope,
                connect: None,
            })
        }
        Ui::Tui => match args.tui_placement {
            TuiPlacement::Current => {
                let server = DiffServer::open(&root, ServerOptions::default()).await?;
                let transport = server.connect()?;
                let client = DiffClient::from_transport(transport, args.scope.into()).await?;
                let outcome = tui::run_client(&client);
                client.close().await?;
                server.shutdown().await?;
                outcome?
            }
            TuiPlacement::External => {
                session::run(&root, args.scope, |socket_path| {
                    tui::launch(socket_path).map_err(|error| error.to_string())
                })
                .await?
            }
        },
    };
    Ok(diff_response(root, args.scope, submission))
}

fn diff_response(
    repository_root: PathBuf,
    scope: DiffScope,
    submission: Option<ReviewSubmission>,
) -> ReviewResponse {
    ReviewResponse::Diff {
        protocol_version: PROTOCOL_VERSION,
        outcome: diff_outcome(submission.as_ref()),
        repository_root,
        scope,
        submission,
    }
}

fn diff_outcome(submission: Option<&ReviewSubmission>) -> ReviewOutcome {
    submission.map_or(ReviewOutcome::Cancelled, |submission| {
        if submission.comments.is_empty() {
            ReviewOutcome::Approved
        } else {
            ReviewOutcome::ChangesRequested
        }
    })
}

fn run_markdown(args: &MarkdownArgs) -> Result<ReviewResponse, AppError> {
    let (source, physical_source_path, file_name) = if args.path == "-" {
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
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        (decode_markdown(bytes)?, Some(args.path.clone()), file_name)
    };
    let source_path = args.source_path.clone().or(physical_source_path);
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
        #[cfg(feature = "desktop")]
        Ui::Desktop => clankerdiff_gpui_desktop::run_markdown_review(document),
        #[cfg(not(feature = "desktop"))]
        Ui::Desktop => return Err(AppError::DesktopDisabled),
    };
    let outcome =
        submission
            .as_ref()
            .map_or(ReviewOutcome::Cancelled, |submission| {
                match submission.decision {
                    MarkdownReviewDecision::Approved => ReviewOutcome::Approved,
                    MarkdownReviewDecision::ChangesRequested => ReviewOutcome::ChangesRequested,
                }
            });
    Ok(ReviewResponse::Markdown {
        protocol_version: PROTOCOL_VERSION,
        outcome,
        source_path,
        submission,
    })
}

fn decode_markdown(bytes: Vec<u8>) -> Result<String, AppError> {
    String::from_utf8(bytes).map_err(|error| AppError::InvalidMarkdownUtf8(error.utf8_error()))
}

fn write_review_response(response: &ReviewResponse, format: OutputFormat) -> Result<(), AppError> {
    match format {
        OutputFormat::Json => write_json(response),
        OutputFormat::Text => {
            match response {
                ReviewResponse::Diff {
                    submission: Some(submission),
                    ..
                } => println!("{}", text_feedback(submission)),
                ReviewResponse::Markdown {
                    submission: Some(submission),
                    ..
                } => println!("{}", markdown_text_feedback(submission)),
                ReviewResponse::Diff {
                    submission: None, ..
                }
                | ReviewResponse::Markdown {
                    submission: None, ..
                } => {}
            }
            Ok(())
        }
    }
}

fn write_capabilities(args: &CapabilitiesArgs) -> Result<(), AppError> {
    let capabilities = CapabilityResponse::default();
    match args.format {
        OutputFormat::Json => write_json(&capabilities),
        OutputFormat::Text => {
            let review_kinds = capabilities
                .review_kinds
                .iter()
                .map(|kind| kind.as_str())
                .collect::<Vec<_>>()
                .join(",");
            let uis = capabilities
                .uis
                .iter()
                .map(|ui| ui.as_str())
                .collect::<Vec<_>>()
                .join(",");
            println!(
                "protocol={} review_kinds={review_kinds} uis={uis} current_terminal_tui={}",
                capabilities.protocol_version, capabilities.current_terminal_tui
            );
            Ok(())
        }
    }
}

/// Writes exactly one compact JSON object followed by one newline.
fn write_json(value: &impl serde::Serialize) -> Result<(), AppError> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, value)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

fn exit_code(outcome: ReviewOutcome) -> ExitCode {
    match outcome {
        ReviewOutcome::Approved | ReviewOutcome::ChangesRequested => ExitCode::SUCCESS,
        ReviewOutcome::Cancelled => ExitCode::from(2),
        _ => ExitCode::FAILURE,
    }
}

fn markdown_text_feedback(submission: &MarkdownReviewSubmission) -> &str {
    match submission.decision {
        MarkdownReviewDecision::Approved => {
            "I reviewed the Markdown document and approve it with no comments."
        }
        MarkdownReviewDecision::ChangesRequested => &submission.formatted,
    }
}

fn text_feedback(submission: &ReviewSubmission) -> &str {
    if submission.comments.is_empty() {
        "I reviewed the working tree diff and approve it with no comments."
    } else {
        &submission.formatted
    }
}

#[derive(Debug, Error)]
enum AppError {
    #[error(transparent)]
    Server(#[from] clankerdiff_server::ServerError),
    #[error(transparent)]
    Client(#[from] clankerdiff_client::ClientError),
    #[cfg(not(feature = "desktop"))]
    #[error("desktop UI is disabled in this build")]
    DesktopDisabled,
    #[error(transparent)]
    Git(#[from] clankerdiff_git::GitError),
    #[error("Markdown input path is a directory: {0}")]
    MarkdownDirectory(String),
    #[error("Markdown input is not valid UTF-8: {0}")]
    InvalidMarkdownUtf8(std::str::Utf8Error),
    #[error(
        "Markdown from stdin cannot use the TUI; use a temporary file so stdin remains interactive"
    )]
    MarkdownStdinTui,
    #[error("could not read or write review data: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Tui(#[from] tui::TuiError),
    #[error(transparent)]
    Session(#[from] session::SessionError),
    #[error("could not serialize review output: {0}")]
    Serialize(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use crate::{markdown_text_feedback, text_feedback};
    use clankerdiff_core::Review;
    use clankerdiff_markdown::{MarkdownReview, MarkdownReviewDecision};

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
