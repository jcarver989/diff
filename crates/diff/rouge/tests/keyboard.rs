mod common;

use clankerdiff_core::{DiffReviewEvent, InteractionPhase, RepositoryAction, ViewMode};
use clankerdiff_rouge::{ReviewOutput, ViewerPane};
use rouge_core::ImeEvent;

#[test]
fn keyboard_navigation_resolves_the_active_pane_and_layout() {
    common::run(common::document(), |ui, handle, _| {
        assert_eq!(handle.selection().get().pane, ViewerPane::Diff);
        let initial = handle.selection().get();
        ui.press("end");
        assert_ne!(handle.selection().get().row, initial.row);
        ui.press("tab");
        assert_eq!(handle.selection().get().pane, ViewerPane::Files);
        ui.press("home");
        ui.press("enter");
        ui.press("down");
        ui.press("enter");
        ui.type_text("v");
        assert_eq!(handle.selection().get().pane, ViewerPane::Diff);
        assert_ne!(handle.status().get().view_mode, ViewMode::Auto);
    });
}

#[test]
fn draft_consumes_typing_and_escape_and_handles_composition() {
    common::run(common::document(), |ui, handle, outputs| {
        ui.type_text("c");
        assert_eq!(handle.status().get().context.phase, InteractionPhase::Draft);
        ui.type_text("ov");
        ui.press("shift-enter");
        ui.ime(ImeEvent::Preedit { text: "漢".to_owned(), cursor: None });
        ui.press("enter");
        assert_eq!(handle.status().get().context.phase, InteractionPhase::Draft);
        ui.ime(ImeEvent::Commit("漢".to_owned()));
        ui.press("enter");
        assert_eq!(handle.status().get().context.phase, InteractionPhase::Browse);
        assert_eq!(
            handle.with(|state| state.session().review().comments().next().map(|c| c.body.clone())).as_deref(),
            Some("ov\n漢")
        );
        assert_eq!(handle.status().get().view_mode, ViewMode::Auto);
        ui.type_text("e");
        ui.press("escape");
        assert!(outputs.borrow().is_empty());
        ui.press("escape");
        assert_eq!(outputs.borrow().as_slice(), &[ReviewOutput::Event(DiffReviewEvent::Cancel)]);
    });
}

#[test]
fn repository_prompt_consumes_keys_and_confirms_only_once() {
    common::run(common::document(), |ui, handle, outputs| {
        ui.type_text("d");
        assert_eq!(handle.status().get().context.phase, InteractionPhase::RepositoryPrompt);
        ui.type_text("n");
        assert!(outputs.borrow().is_empty());
        assert_eq!(handle.status().get().context.phase, InteractionPhase::Browse);
        ui.type_text("d");
        ui.type_text("y");
        assert!(matches!(
            outputs.borrow().as_slice(),
            [ReviewOutput::Event(DiffReviewEvent::RepositoryAction(RepositoryAction::Discard { .. }))]
        ));
        ui.type_text("d");
        assert_eq!(handle.status().get().context.phase, InteractionPhase::Browse);
    });
}

#[test]
fn theme_preview_cancels_locally_and_commit_notifies_the_host() {
    common::run(common::document(), |ui, handle, outputs| {
        let original = handle.theme().get().revision();
        ui.type_text("t");
        ui.press("down");
        assert_ne!(handle.theme().get().revision(), original);
        ui.press("escape");
        assert_eq!(handle.theme().get().revision(), original);
        assert!(outputs.borrow().is_empty());
        ui.type_text("t");
        ui.press("down");
        ui.press("enter");
        assert!(matches!(outputs.borrow().as_slice(), [ReviewOutput::ThemeChanged(_)]));
        assert_eq!(handle.status().get().context.phase, InteractionPhase::Browse);
    });
}

#[test]
fn native_shift_slash_opens_help_and_resize_updates_automatic_layout() {
    common::run(common::document(), |ui, handle, outputs| {
        assert!(!handle.status().get().layout.is_split());
        ui.press("shift-/");
        assert_eq!(handle.status().get().context.phase, InteractionPhase::Help);
        ui.press("escape");
        ui.set_size(rouge_core::Size::new(1600.0, 900.0));
        ui.advance(0.0);
        assert!(handle.status().get().layout.is_split());
        ui.set_size(rouge_core::Size::new(600.0, 800.0));
        ui.advance(0.0);
        assert!(!handle.status().get().layout.is_split());
        assert!(outputs.borrow().is_empty());
    });
}

#[test]
fn enter_expands_context_but_o_switches_to_source() {
    use clankerdiff_core::{RowKind, testing::DocumentBuilder};

    let document = DocumentBuilder::new()
        .changed_with_hunk_window("a.rs", "one\ntwo\nthree\nfour\nfive\n", "one\ntwo\nTHREE\nfour\nfive\n", 2..=4)
        .build();
    common::run(document, |ui, handle, _| {
        ui.press("home");
        ui.press("down");
        ui.press("enter");
        assert!(handle.with(|state| {
            let session = state.session();
            let presentation = session.presentation();
            !session.shows_source()
                && presentation.rows(0..presentation.row_count()).iter().any(|row| row.kind == RowKind::ExpandedContext)
        }));
        ui.type_text("o");
        assert!(handle.with(|state| state.session().shows_source()));
        ui.type_text("o");
        assert!(!handle.with(|state| state.session().shows_source()));
    });
}
