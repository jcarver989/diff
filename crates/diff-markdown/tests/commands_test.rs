#![cfg(feature = "review")]

use clankerdiff_core::{CommandContext, InteractionPhase, ReviewCapabilities, ReviewCommand};
use clankerdiff_markdown::{MarkdownFocusPane, MarkdownReviewCommand, MarkdownReviewDecision};

#[test]
fn decisions_respect_capabilities_and_drafts() {
    for context in [
        CommandContext {
            capabilities: ReviewCapabilities {
                submit: false,
                clipboard: false,
                ..ReviewCapabilities::default()
            },
            ..CommandContext::default()
        },
        CommandContext {
            phase: InteractionPhase::Draft,
            ..CommandContext::default()
        },
        CommandContext {
            phase: InteractionPhase::Help,
            ..CommandContext::default()
        },
        CommandContext {
            phase: InteractionPhase::ThemePicker,
            ..CommandContext::default()
        },
        CommandContext {
            document_ready: false,
            ..CommandContext::default()
        },
    ] {
        for command in [
            MarkdownReviewCommand::Approve,
            MarkdownReviewCommand::RequestChanges,
            MarkdownReviewCommand::CopyReview(MarkdownReviewDecision::Approved),
        ] {
            assert!(!command.enabled(&context));
            assert!(command.enabled(&CommandContext::default()));
        }
        assert!(MarkdownReviewCommand::from(ReviewCommand::Cancel).enabled(&context));
    }
}

#[test]
fn hidden_outline_does_not_disable_document_commands() {
    let context = CommandContext {
        navigation_available: false,
        ..CommandContext::default()
    };
    assert!(!MarkdownReviewCommand::ToggleFocus.enabled(&context));
    assert!(!MarkdownReviewCommand::Focus(MarkdownFocusPane::Outline).enabled(&context));
    assert!(MarkdownReviewCommand::Focus(MarkdownFocusPane::Document).enabled(&context));
    assert!(MarkdownReviewCommand::from(ReviewCommand::BeginComment).enabled(&context));
}
