#![cfg(feature = "test-support")]

use clankerdiff_core::testing::DocumentBuilder;
use clankerdiff_core::{
    DiffReviewCommand, DiffReviewEvent, InteractionPhase, ReviewCapabilities, ReviewCommand,
};
use clankerdiff_gpui::testing::DiffViewerHarnessBuilder;
use clankerdiff_gpui::{MarkdownReviewer, NextFile, NextHunk, Refresh, StageAll};
use clankerdiff_markdown::{MarkdownDocument, MarkdownReviewCommand, MarkdownReviewEvent};
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
fn refresh_shortcuts_obey_pending_and_modal_state(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let harness = DiffViewerHarnessBuilder::default().build(cx);
        harness.simulate_keystrokes(cx, "cmd-r");
        assert_eq!(harness.events(cx), vec![DiffReviewEvent::Refresh]);
        harness.update(cx, |viewer, cx| viewer.set_repository_pending(true, cx));
        harness.simulate_keystrokes(cx, "ctrl-r");
        assert!(!harness.dispatch_command(cx, DiffReviewCommand::Refresh)?);
        harness.update(cx, |viewer, cx| viewer.set_repository_pending(false, cx));
        harness.dispatch_command(cx, ReviewCommand::BeginComment)?;
        harness.dispatch_action(cx, &Refresh)?;
        assert_eq!(harness.events(cx), vec![DiffReviewEvent::Refresh]);
        harness.dispatch_command(cx, ReviewCommand::Cancel)?;
        harness.simulate_keystrokes(cx, "ctrl-r");
        assert_eq!(
            harness.events(cx),
            vec![DiffReviewEvent::Refresh, DiffReviewEvent::Refresh]
        );
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn menu_navigation_actions_cannot_bypass_a_draft(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let harness = DiffViewerHarnessBuilder {
            document: DocumentBuilder::new()
                .changed("a.rs", "old\n", "new\n")
                .changed("b.rs", "before\n", "after\n")
                .build(),
            ..DiffViewerHarnessBuilder::default()
        }
        .build(cx);
        let selected = harness.read(cx, |viewer, _| {
            (viewer.selected_file(), viewer.session().selected_row())
        });
        harness.dispatch_command(cx, ReviewCommand::BeginComment)?;
        harness.dispatch_action(cx, &NextFile)?;
        harness.dispatch_action(cx, &NextHunk)?;
        harness.dispatch_action(cx, &StageAll)?;
        assert_eq!(
            harness.read(cx, |viewer, _| (
                viewer.selected_file(),
                viewer.session().selected_row()
            )),
            selected
        );
        assert_eq!(
            harness.read(cx, |viewer, _| viewer.command_context().phase),
            InteractionPhase::Draft
        );
        assert!(harness.events(cx).is_empty());
        harness.dispatch_command(cx, ReviewCommand::Cancel)?;
        harness.dispatch_action(cx, &NextFile)?;
        assert_ne!(
            harness.read(cx, |viewer, _| viewer.selected_file()),
            selected.0
        );
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn theme_keyboard_preview_and_commit_match_shared_commands(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let direct = DiffViewerHarnessBuilder::default().build(cx);
        direct.dispatch_command(cx, ReviewCommand::OpenThemePicker)?;
        direct.dispatch_command(cx, ReviewCommand::MoveTheme(1))?;
        let preview = direct.read(cx, |viewer, _| viewer.theme().id().clone());
        let keyboard = DiffViewerHarnessBuilder::default().build(cx);
        let original = keyboard.read(cx, |viewer, _| viewer.theme().id().clone());
        keyboard.simulate_keystrokes(cx, "t down");
        assert_eq!(
            keyboard.read(cx, |viewer, _| viewer.theme().id().clone()),
            preview
        );
        keyboard.simulate_keystrokes(cx, "escape");
        assert_eq!(
            keyboard.read(cx, |viewer, _| viewer.theme().id().clone()),
            original
        );
        keyboard.simulate_keystrokes(cx, "t j enter");
        assert_eq!(
            keyboard.read(cx, |viewer, _| viewer.theme().id().clone()),
            preview
        );
        assert_eq!(
            keyboard.read(cx, |viewer, _| viewer.command_context().phase),
            InteractionPhase::Browse
        );
        assert!(keyboard.events(cx).is_empty());
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn markdown_navigation_and_theme_shortcuts_use_shared_commands(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        cx.update(MarkdownReviewer::bind_keys);
        let document = Arc::new(MarkdownDocument::parse("# One\n\nText\n\n# Two\n\nMore"));
        let second_heading = document.outline()[1].target_id;
        let keyboard = cx.add_window(|_, _| MarkdownReviewer::new(document));
        cx.update_window(*keyboard, |_, window, cx| window.draw(cx).clear(cx))?;
        keyboard.update(cx, |viewer, window, cx| {
            viewer.focus_handle(cx).focus(window, cx)
        })?;
        cx.simulate_keystrokes(*keyboard, "tab j enter");
        assert_eq!(
            keyboard.read_with(cx, |viewer, _| viewer.session().selected_target())?,
            Some(second_heading)
        );
        let events = cx.new(|_| Vec::<MarkdownReviewEvent>::new());
        keyboard.update(cx, |_, _, cx| {
            let recorded = events.clone();
            cx.subscribe(
                &cx.entity(),
                move |_, _, event: &MarkdownReviewEvent, cx| {
                    recorded.update(cx, |events, _| events.push(event.clone()));
                },
            )
            .detach();
        })?;
        cx.simulate_keystrokes(*keyboard, "y");
        assert!(events.read_with(cx, |events, _| matches!(
            events.last(),
            Some(MarkdownReviewEvent::CopyFormatted(_))
        )));
        keyboard.update(cx, |viewer, _, cx| {
            viewer.set_capabilities(
                ReviewCapabilities {
                    clipboard: false,
                    ..ReviewCapabilities::default()
                },
                cx,
            );
        })?;
        cx.simulate_keystrokes(*keyboard, "y");
        assert_eq!(events.read_with(cx, |events, _| events.len()), 1);
        let before = keyboard.read_with(cx, |viewer, _| viewer.session().selected_target())?;
        cx.simulate_keystrokes(*keyboard, "pagedown");
        assert_ne!(
            keyboard.read_with(cx, |viewer, _| viewer.session().selected_target())?,
            before
        );
        cx.simulate_keystrokes(*keyboard, "t down enter");
        assert_eq!(
            keyboard.read_with(cx, |viewer, _| viewer.command_context().phase)?,
            InteractionPhase::Browse
        );
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
