use clankerdiff_core::{
    CommandContext, DiffReviewCommand, DiffScope, FocusPane, InteractionPhase, RepositoryAction,
    ReviewCapabilities, ReviewCommand,
};

#[test]
fn repository_commands_require_capabilities_and_browse_mode() {
    for context in [
        CommandContext {
            repository_pending: true,
            ..CommandContext::default()
        },
        CommandContext {
            capabilities: ReviewCapabilities {
                repository: false,
                ..ReviewCapabilities::default()
            },
            ..CommandContext::default()
        },
        CommandContext {
            phase: InteractionPhase::Help,
            ..CommandContext::default()
        },
        CommandContext {
            phase: InteractionPhase::Draft,
            ..CommandContext::default()
        },
        CommandContext {
            phase: InteractionPhase::RepositoryPrompt,
            ..CommandContext::default()
        },
        CommandContext {
            phase: InteractionPhase::ThemePicker,
            ..CommandContext::default()
        },
    ] {
        for command in [
            DiffReviewCommand::StageAll,
            DiffReviewCommand::BeginCommit,
            DiffReviewCommand::RepositoryAction(RepositoryAction::UnstageAll),
        ] {
            assert!(!command.enabled(&context));
        }
        assert!(ReviewCommand::Cancel.enabled(&context));
    }
}

#[test]
fn loading_blocks_review_mutations_but_allows_refresh_and_scope_changes() {
    let context = CommandContext {
        document_ready: false,
        ..CommandContext::default()
    };
    assert!(!ReviewCommand::BeginComment.enabled(&context));
    assert!(!DiffReviewCommand::SubmitReview.enabled(&context));
    for command in [
        DiffReviewCommand::Refresh,
        DiffReviewCommand::CycleScope,
        DiffReviewCommand::SetScope(DiffScope::Staged),
    ] {
        assert!(command.enabled(&context));
    }
}

#[test]
fn host_capabilities_gate_each_host_intent() {
    let context = CommandContext {
        capabilities: ReviewCapabilities {
            repository: false,
            refresh: false,
            scope: false,
            submit: false,
            clipboard: false,
        },
        ..CommandContext::default()
    };
    for command in [
        DiffReviewCommand::StageAll,
        DiffReviewCommand::Refresh,
        DiffReviewCommand::CycleScope,
        DiffReviewCommand::SubmitReview,
        DiffReviewCommand::CopyReview,
    ] {
        assert!(!command.enabled(&context));
        assert!(command.enabled(&CommandContext::default()));
    }
}

#[test]
fn modal_commands_are_available_only_in_their_phase() {
    for (command, phase) in [
        (ReviewCommand::SubmitComment, InteractionPhase::Draft),
        (ReviewCommand::ScrollHelp(1), InteractionPhase::Help),
        (ReviewCommand::SelectTheme(0), InteractionPhase::ThemePicker),
        (ReviewCommand::MoveTheme(1), InteractionPhase::ThemePicker),
        (ReviewCommand::CommitTheme, InteractionPhase::ThemePicker),
    ] {
        assert!(!command.enabled(&CommandContext::default()));
        assert!(command.enabled(&CommandContext {
            phase,
            ..CommandContext::default()
        }));
    }
}

#[test]
fn unavailable_navigation_and_themes_do_not_disable_document_commands() {
    let context = CommandContext {
        navigation_available: false,
        ..CommandContext::default()
    };
    assert!(!DiffReviewCommand::ToggleFocus.enabled(&context));
    assert!(!DiffReviewCommand::Focus(FocusPane::Files).enabled(&context));
    assert!(DiffReviewCommand::Focus(FocusPane::Diff).enabled(&context));
    assert!(ReviewCommand::BeginComment.enabled(&context));
    assert!(!ReviewCommand::OpenThemePicker.enabled(&context));
    assert!(ReviewCommand::OpenThemePicker.enabled(&CommandContext {
        themes_available: true,
        ..context
    }));
}
