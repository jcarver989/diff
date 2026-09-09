#![cfg(feature = "test-support")]

use clankerdiff_core::{
    DiffReviewCommand, DiffReviewEvent, InteractionPhase, ReviewCapabilities, ReviewCommand,
};
use clankerdiff_gpui::MarkdownReviewer;
use clankerdiff_gpui::testing::DiffViewerHarnessBuilder;
use clankerdiff_markdown::{MarkdownDocument, MarkdownReviewCommand};
use clankerdiff_theme::ReviewTheme;
use gpui::{AppContext, Focusable, TestAppContext};
use std::{error::Error, sync::Arc};

#[gpui::test]
fn keyboard_and_shared_commands_create_identical_reviews(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let keyboard = DiffViewerHarnessBuilder::default().build(cx);
        keyboard.simulate_keystrokes(cx, "c h e l l o enter");
        let direct = DiffViewerHarnessBuilder::default().build(cx);
        assert!(direct.dispatch_command(cx, ReviewCommand::BeginComment)?);
        direct.update(cx, |viewer, _| -> Result<(), Box<dyn Error>> {
            viewer
                .session_mut()
                .draft_mut()
                .ok_or("missing draft")?
                .set_body("hello");
            Ok(())
        })?;
        assert!(direct.dispatch_command(cx, ReviewCommand::SubmitComment)?);
        assert_eq!(
            keyboard.read(cx, |viewer, _| viewer.review().clone()),
            direct.read(cx, |viewer, _| viewer.review().clone())
        );
        assert!(direct.dispatch_command(cx, DiffReviewCommand::SubmitReview)?);
        assert!(matches!(
            direct.events(cx).last(),
            Some(DiffReviewEvent::SubmitReview(_))
        ));
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn direct_commands_and_shortcuts_obey_the_same_host_capabilities(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let harness = DiffViewerHarnessBuilder::default().build(cx);
        harness.update(cx, |viewer, cx| {
            viewer.set_capabilities(
                ReviewCapabilities {
                    repository: false,
                    refresh: false,
                    scope: false,
                    submit: false,
                    clipboard: false,
                },
                cx,
            )
        });
        for command in [
            DiffReviewCommand::StageAll,
            DiffReviewCommand::BeginCommit,
            DiffReviewCommand::Refresh,
            DiffReviewCommand::CycleScope,
            DiffReviewCommand::SubmitReview,
            DiffReviewCommand::CopyReview,
        ] {
            assert!(!harness.read(cx, |viewer, _| viewer.command_enabled(&command)));
            assert!(!harness.dispatch_command(cx, command)?);
        }
        harness.simulate_keystrokes(cx, "h a shift-a shift-c d shift-s tab s y");
        assert!(harness.events(cx).is_empty());
        assert!(harness.dispatch_command(cx, ReviewCommand::Cancel)?);
        assert_eq!(harness.events(cx), vec![DiffReviewEvent::Cancel]);
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn direct_comment_commands_close_blank_drafts_and_preserve_edits(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let harness = DiffViewerHarnessBuilder::default().build(cx);
        assert!(!harness.dispatch_command(cx, ReviewCommand::EditComment)?);
        assert!(!harness.dispatch_command(cx, ReviewCommand::DeleteComment)?);
        assert!(!harness.dispatch_command(cx, ReviewCommand::UndoComment)?);
        assert!(harness.dispatch_command(cx, ReviewCommand::BeginComment)?);
        assert!(harness.dispatch_command(cx, ReviewCommand::SubmitComment)?);
        assert_eq!(
            harness.read(cx, |viewer, _| viewer.command_context().phase),
            InteractionPhase::Browse
        );
        assert!(harness.read(cx, |viewer, _| viewer.review().is_empty()));
        harness.simulate_keystrokes(cx, "c h e l l o enter");
        let original = harness.read(cx, |viewer, _| viewer.review().clone());
        assert_eq!(original.len(), 1);
        assert!(harness.dispatch_command(cx, ReviewCommand::EditComment)?);
        harness.update(cx, |viewer, _| -> Result<(), Box<dyn Error>> {
            viewer
                .session_mut()
                .draft_mut()
                .ok_or("missing edit draft")?
                .set_body("edited");
            Ok(())
        })?;
        assert!(!harness.dispatch_command(cx, ReviewCommand::DeleteComment)?);
        assert!(harness.dispatch_command(cx, ReviewCommand::Cancel)?);
        assert_eq!(
            harness.read(cx, |viewer, _| viewer.review().clone()),
            original
        );
        assert!(harness.dispatch_command(cx, ReviewCommand::EditComment)?);
        harness.update(cx, |viewer, _| -> Result<(), Box<dyn Error>> {
            viewer
                .session_mut()
                .draft_mut()
                .ok_or("missing edit draft")?
                .set_body("edited");
            Ok(())
        })?;
        assert!(harness.dispatch_command(cx, ReviewCommand::SubmitComment)?);
        assert_eq!(harness.read(cx, |viewer, _| viewer.review().len()), 1);
        assert_ne!(
            harness.read(cx, |viewer, _| viewer.review().clone()),
            original
        );
        assert!(harness.dispatch_command(cx, ReviewCommand::DeleteComment)?);
        assert!(harness.read(cx, |viewer, _| viewer.review().is_empty()));
        harness.simulate_keystrokes(cx, "c n e w enter");
        assert!(harness.dispatch_command(cx, ReviewCommand::UndoComment)?);
        assert!(harness.read(cx, |viewer, _| viewer.review().is_empty()));
        assert!(harness.events(cx).is_empty());
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn markdown_keyboard_and_direct_commands_share_draft_state(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        cx.update(MarkdownReviewer::bind_keys);
        let document = Arc::new(MarkdownDocument::parse("# Heading\n\nText"));
        let keyboard = cx.add_window(|_, _| MarkdownReviewer::new(document.clone()));
        cx.update_window(*keyboard, |_, window, cx| window.draw(cx).clear(cx))?;
        keyboard.update(cx, |viewer, window, cx| {
            viewer.focus_handle(cx).focus(window, cx)
        })?;
        cx.simulate_keystrokes(*keyboard, "c h e l l o enter");
        let direct = cx.add_window(|_, _| MarkdownReviewer::new(document));
        direct.update(cx, |viewer, window, cx| -> Result<(), Box<dyn Error>> {
            assert!(!viewer.handle_command(ReviewCommand::EditComment, window, cx)?);
            assert!(viewer.handle_command(ReviewCommand::BeginComment, window, cx)?);
            assert!(!viewer.handle_command(MarkdownReviewCommand::NextHeading, window, cx)?);
            viewer
                .session_mut()
                .draft_mut()
                .ok_or("missing draft")?
                .set_body("hello");
            assert!(viewer.handle_command(ReviewCommand::SubmitComment, window, cx)?);
            assert_eq!(viewer.command_context().phase, InteractionPhase::Browse);
            Ok(())
        })??;
        let expected = keyboard.read_with(cx, |viewer, _| viewer.review().clone())?;
        assert_eq!(expected.len(), 1);
        assert_eq!(
            direct.read_with(cx, |viewer, _| viewer.review().clone())?,
            expected
        );
        direct.update(cx, |viewer, window, cx| -> Result<(), Box<dyn Error>> {
            assert!(viewer.handle_command(ReviewCommand::UndoComment, window, cx)?);
            assert!(viewer.review().is_empty());
            assert!(viewer.handle_command(ReviewCommand::BeginComment, window, cx)?);
            assert!(viewer.handle_command(ReviewCommand::SubmitComment, window, cx)?);
            assert_eq!(viewer.command_context().phase, InteractionPhase::Browse);
            assert!(viewer.review().is_empty());
            Ok(())
        })??;
        cx.update_window(*direct, |_, window, cx| window.draw(cx).clear(cx))?;
        direct.update(cx, |viewer, window, cx| {
            viewer.focus_handle(cx).focus(window, cx)
        })?;
        cx.simulate_keystrokes(*direct, "c n e w enter");
        assert_eq!(direct.read_with(cx, |viewer, _| viewer.review().len())?, 1);
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn help_and_repository_modals_block_commands_and_cancel_locally(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let harness = DiffViewerHarnessBuilder::default().build(cx);
        for (command, phase) in [
            (ReviewCommand::ShowHelp.into(), InteractionPhase::Help),
            (
                DiffReviewCommand::BeginCommit,
                InteractionPhase::RepositoryPrompt,
            ),
        ] {
            assert!(harness.dispatch_command(cx, command)?);
            assert_eq!(
                harness.read(cx, |viewer, _| viewer.command_context().phase),
                phase
            );
            assert!(!harness.dispatch_command(cx, DiffReviewCommand::Refresh)?);
            assert!(!harness.dispatch_command(cx, ReviewCommand::BeginComment)?);
            harness.simulate_keystrokes(cx, "escape");
            assert_eq!(
                harness.read(cx, |viewer, _| viewer.command_context().phase),
                InteractionPhase::Browse
            );
            assert!(harness.events(cx).is_empty());
        }
        assert!(harness.dispatch_command(cx, ReviewCommand::Cancel)?);
        assert_eq!(harness.events(cx), vec![DiffReviewEvent::Cancel]);
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn theme_preview_cancellation_is_shared_and_does_not_cancel_the_review(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let harness = DiffViewerHarnessBuilder::default().build(cx);
        let original = harness.read(cx, |viewer, _| viewer.theme().id().clone());
        assert!(harness.dispatch_command(cx, ReviewCommand::OpenThemePicker)?);
        assert!(harness.read(cx, |viewer, _| viewer.theme_picker_open()));
        assert_eq!(
            harness.read(cx, |viewer, _| viewer.command_context().phase),
            InteractionPhase::ThemePicker
        );
        assert!(harness.dispatch_command(cx, ReviewCommand::SelectTheme(0))?);
        assert!(harness.dispatch_command(cx, ReviewCommand::Cancel)?);
        assert_eq!(
            harness.read(cx, |viewer, _| viewer.theme().id().clone()),
            original
        );
        assert!(!harness.read(cx, |viewer, _| viewer.theme_picker_open()));
        assert!(harness.events(cx).is_empty());
        assert!(harness.dispatch_command(cx, ReviewCommand::OpenThemePicker)?);
        assert!(harness.dispatch_command(cx, ReviewCommand::CommitTheme)?);
        assert!(!harness.read(cx, |viewer, _| viewer.theme_picker_open()));
        assert!(harness.dispatch_command(cx, ReviewCommand::OpenThemePicker)?);
        harness.update(cx, |viewer, cx| {
            viewer.set_theme(ReviewTheme::builder("host override").build(), cx)
        });
        assert!(!harness.read(cx, |viewer, _| viewer.theme_picker_open()));
        assert_eq!(
            harness.read(cx, |viewer, _| viewer.command_context().phase),
            InteractionPhase::Browse
        );
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}
