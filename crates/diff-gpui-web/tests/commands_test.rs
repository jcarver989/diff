use clankerdiff_core::{DiffReviewCommand, FocusPane, ReviewCommand};
use clankerdiff_gpui_web::commands::{
    CommandError, CommandRequest, ViewerCommand, decode_capabilities, decode_command,
};
use clankerdiff_markdown::{MarkdownReviewCommand, MarkdownReviewDecision};
use std::error::Error;

#[test]
fn decodes_shared_commands_without_a_browser() -> Result<(), Box<dyn Error>> {
    for (json, command) in [
        (
            r#"{"target":"diff","command":"refresh"}"#,
            ViewerCommand::Diff(DiffReviewCommand::Refresh),
        ),
        (
            r#"{"target":"diff","command":{"review":"begin_comment"}}"#,
            ViewerCommand::Diff(ReviewCommand::BeginComment.into()),
        ),
        (
            r#"{"target":"diff","command":{"scroll":{"pane":"diff","lines":-3}}}"#,
            ViewerCommand::Diff(DiffReviewCommand::Scroll {
                pane: FocusPane::Diff,
                lines: -3,
            }),
        ),
        (
            r#"{"target":"markdown","command":"approve"}"#,
            ViewerCommand::Markdown(MarkdownReviewCommand::Approve),
        ),
    ] {
        assert_eq!(
            decode_command(json)?,
            CommandRequest {
                request_id: None,
                command
            }
        );
    }
    Ok(())
}

#[test]
fn requests_round_trip_with_correlation_ids() -> Result<(), Box<dyn Error>> {
    for command in [
        ViewerCommand::Diff(DiffReviewCommand::SelectFile(3)),
        ViewerCommand::Diff(ReviewCommand::MoveTheme(-1).into()),
        ViewerCommand::Markdown(MarkdownReviewCommand::CopyReview(
            MarkdownReviewDecision::ChangesRequested,
        )),
    ] {
        let request = CommandRequest {
            request_id: Some(42),
            command,
        };
        assert_eq!(decode_command(&serde_json::to_string(&request)?)?, request);
    }
    Ok(())
}

#[test]
fn rejects_invalid_commands_and_capabilities() {
    for json in [
        r#"{"target":"other","command":"refresh"}"#,
        r#"{"target":"markdown","command":"stage_all"}"#,
        r#"{"target":"diff","command":{"select_file":-1}}"#,
        r#"{"target":"diff","command":{"review":"not_a_command"}}"#,
        r#"{"target":"diff"}"#,
        "not json",
    ] {
        assert!(matches!(
            decode_command(json),
            Err(CommandError::InvalidCommand(_))
        ));
    }
    for json in ["{}", r#"{"repository":"false"}"#, "null"] {
        assert!(matches!(
            decode_capabilities(json),
            Err(CommandError::InvalidCapabilities(_))
        ));
    }
}

#[test]
fn decodes_host_capabilities_for_shared_command_gates() -> Result<(), Box<dyn Error>> {
    let capabilities = decode_capabilities(
        r#"{"repository":false,"refresh":true,"scope":false,"submit":true,"clipboard":false}"#,
    )?;
    assert!(!capabilities.repository);
    assert!(capabilities.refresh);
    assert!(!capabilities.scope);
    assert!(capabilities.submit);
    assert!(!capabilities.clipboard);
    Ok(())
}
