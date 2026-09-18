use clankerdiff_client::{ClientOptions, DiffClient, DiffReviewEvent};
use clankerdiff_core::{DiffSide, LineAnchor, Review};
use clankerdiff_git::testing::RepoFixtureBuilder;
use clankerdiff_protocol::{ReviewOutcome, ReviewResponse, parse_response};
use std::{
    error::Error,
    io::{BufRead, BufReader},
    process::{Child, Command, Output, Stdio},
    time::Duration,
};

#[test]
fn remote_commands_are_available_in_executable() -> Result<(), Box<dyn Error>> {
    for command in ["serve", "connect"] {
        let output = Command::new(env!("CARGO_BIN_EXE_clankerdiff"))
            .args([command, "--help"])
            .output()?;
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout)?;
        assert!(text.contains(command));
        if command == "serve" {
            assert!(text.contains("127.0.0.1:7331"));
            assert!(text.contains("--format"));
        } else {
            assert!(text.contains("--ui"));
            assert!(text.contains("--scope"));
            assert!(!text.contains("--format"));
        }
    }
    Ok(())
}

#[tokio::test]
async fn server_stdout_receives_remote_submission() -> Result<(), Box<dyn Error>> {
    let repo = RepoFixtureBuilder::new()
        .file("file", "old\n")
        .committed()
        .build();
    repo.write("file", "new\n");
    let repository = repo.repository().await;
    let mut server = RunningServer::spawn(repository.root())?;
    let url = server.url()?;
    let client = DiffClient::connect(&url, ClientOptions::default()).await?;
    let state = client
        .subscribe()
        .wait_until(Duration::from_secs(10), |state| state.snapshot.is_some())
        .await?;
    let file = &state.snapshot.as_ref().ok_or("snapshot")?.document.files[0];
    let anchor = (0..file.hunks[0].lines.len())
        .find_map(|line| LineAnchor::for_line(file, DiffSide::New, 0, line))
        .ok_or("new line")?;
    let mut review = Review::default();
    review.add_comment(anchor, "feedback from machine B");
    let submission = review.submission();
    client
        .handle(DiffReviewEvent::SubmitReview(submission.clone()))
        .await?;
    drop(client);

    let output = server.finish()?;
    assert!(output.status.success());
    let response = parse_response(&output.stdout)?;
    assert!(matches!(
        response,
        ReviewResponse::Diff {
            outcome: ReviewOutcome::ChangesRequested,
            submission: Some(received),
            ..
        } if received == submission
    ));
    Ok(())
}

#[tokio::test]
async fn server_cancellation_uses_the_review_exit_code() -> Result<(), Box<dyn Error>> {
    let repo = RepoFixtureBuilder::new()
        .file("file", "old\n")
        .committed()
        .build();
    let repository = repo.repository().await;
    let mut server = RunningServer::spawn(repository.root())?;
    let client = DiffClient::connect(&server.url()?, ClientOptions::default()).await?;
    client.handle(DiffReviewEvent::Cancel).await?;
    drop(client);

    let output = server.finish()?;
    assert_eq!(output.status.code(), Some(2));
    assert!(matches!(
        parse_response(&output.stdout)?,
        ReviewResponse::Diff {
            outcome: ReviewOutcome::Cancelled,
            submission: None,
            ..
        }
    ));
    Ok(())
}

#[cfg(not(feature = "desktop"))]
#[test]
fn headless_desktop_request_fails_before_network_connection() -> Result<(), Box<dyn Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_clankerdiff"))
        .args(["connect", "ws://127.0.0.1:1/ws", "--ui", "desktop"])
        .output()?;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8(output.stderr)?.contains("desktop"));
    Ok(())
}

struct RunningServer(Option<Child>);

impl RunningServer {
    fn spawn(repository: &std::path::Path) -> Result<Self, Box<dyn Error>> {
        let child = Command::new(env!("CARGO_BIN_EXE_clankerdiff"))
            .args(["serve", "--listen", "127.0.0.1:0", "--format", "json"])
            .arg(repository)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        Ok(Self(Some(child)))
    }

    fn url(&mut self) -> Result<String, Box<dyn Error>> {
        let stderr = self
            .0
            .as_mut()
            .and_then(|child| child.stderr.as_mut())
            .ok_or("server stderr unavailable")?;
        let mut line = String::new();
        BufReader::new(stderr).read_line(&mut line)?;
        line.split_whitespace()
            .find(|word| word.starts_with("ws://"))
            .map(str::to_owned)
            .ok_or_else(|| format!("server did not report a URL: {line}").into())
    }

    fn finish(mut self) -> Result<Output, Box<dyn Error>> {
        let child = self.0.take().ok_or("server already finished")?;
        Ok(child.wait_with_output()?)
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
