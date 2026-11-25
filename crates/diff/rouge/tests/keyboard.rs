mod common;

use clankerdiff_core::{
    DiffReviewEvent, InteractionPhase, RepositoryAction, RowKind, ViewMode, testing::DocumentBuilder,
};
use clankerdiff_rouge::{ReviewOutput, ViewerPane};
use common::ReviewHarness;
use rouge_core::{ImeEvent, Size};

#[test]
fn keyboard_navigation_resolves_the_active_pane_and_layout() {
    let mut review = ReviewHarness::new(common::document());
    assert_eq!(review.handle.selection().get().pane, ViewerPane::Diff);
    let initial = review.handle.selection().get();
    review.press("end");
    assert_ne!(review.handle.selection().get().row, initial.row);
    review.press("tab");
    assert_eq!(review.handle.selection().get().pane, ViewerPane::Files);
    review.press("home");
    review.press("enter");
    review.press("down");
    review.press("enter");
    review.ui.type_text("v");
    assert_eq!(review.handle.selection().get().pane, ViewerPane::Diff);
    assert_ne!(review.handle.status().get().view_mode, ViewMode::Auto);
}

#[test]
fn draft_consumes_typing_and_escape_and_handles_composition() {
    let mut review = ReviewHarness::new(common::document());
    review.ui.type_text("c");
    assert_eq!(review.handle.status().get().context.phase, InteractionPhase::Draft);
    review.ui.type_text("ov");
    review.press("shift-enter");
    review.ui.ime(ImeEvent::Preedit { text: "漢".to_owned(), cursor: None });
    review.press("enter");
    assert_eq!(review.handle.status().get().context.phase, InteractionPhase::Draft);
    review.ui.ime(ImeEvent::Commit("漢".to_owned()));
    review.press("enter");
    assert_eq!(review.handle.status().get().context.phase, InteractionPhase::Browse);
    assert_eq!(
        review.handle.with(|state| state.session().review().comments().next().map(|c| c.body.clone())).as_deref(),
        Some("ov\n漢")
    );
    assert_eq!(review.handle.status().get().view_mode, ViewMode::Auto);
    review.ui.type_text("e");
    review.press("escape");
    assert!(review.outputs().is_empty());
    review.press("escape");
    assert_eq!(&*review.outputs(), &[ReviewOutput::Event(DiffReviewEvent::Cancel)]);
}

#[test]
fn repository_prompt_consumes_keys_and_confirms_only_once() {
    let mut review = ReviewHarness::new(common::document());
    review.ui.type_text("d");
    assert_eq!(review.handle.status().get().context.phase, InteractionPhase::RepositoryPrompt);
    review.ui.type_text("n");
    assert!(review.outputs().is_empty());
    assert_eq!(review.handle.status().get().context.phase, InteractionPhase::Browse);
    review.ui.type_text("d");
    review.ui.type_text("y");
    assert!(matches!(
        &*review.outputs(),
        [ReviewOutput::Event(DiffReviewEvent::RepositoryAction(RepositoryAction::Discard { .. }))]
    ));
    review.ui.type_text("d");
    assert_eq!(review.handle.status().get().context.phase, InteractionPhase::Browse);
}

#[test]
fn theme_preview_cancels_locally_and_commit_notifies_the_host() {
    let mut review = ReviewHarness::new(common::document());
    let original = review.handle.theme().get().revision();
    review.ui.type_text("t");
    review.press("down");
    assert_ne!(review.handle.theme().get().revision(), original);
    review.press("escape");
    assert_eq!(review.handle.theme().get().revision(), original);
    assert!(review.outputs().is_empty());
    review.ui.type_text("t");
    review.press("down");
    review.press("enter");
    assert!(matches!(&*review.outputs(), [ReviewOutput::ThemeChanged(_)]));
    assert_eq!(review.handle.status().get().context.phase, InteractionPhase::Browse);
}

#[test]
fn native_shift_slash_opens_help_and_resize_updates_automatic_layout() {
    let mut review = ReviewHarness::new(common::document());
    assert!(!review.handle.status().get().layout.is_split());
    review.press("shift-/");
    assert_eq!(review.handle.status().get().context.phase, InteractionPhase::Help);
    review.press("escape");
    review.ui.set_size(Size::new(1600.0, 900.0));
    review.ui.advance(0.0);
    assert!(review.handle.status().get().layout.is_split());
    review.ui.set_size(Size::new(600.0, 800.0));
    review.ui.advance(0.0);
    assert!(!review.handle.status().get().layout.is_split());
    assert!(review.outputs().is_empty());
}

#[test]
fn enter_expands_context_but_o_switches_to_source() {
    let document = DocumentBuilder::new()
        .changed_with_hunk_window("a.rs", "one\ntwo\nthree\nfour\nfive\n", "one\ntwo\nTHREE\nfour\nfive\n", 2..=4)
        .build();
    let mut review = ReviewHarness::new(document);
    review.press("home");
    review.press("down");
    review.press("enter");
    review.assert_hidden("source-mode-header");
    assert!(review.handle.with(|state| {
        let presentation = state.session().presentation();
        presentation.rows(0..presentation.row_count()).iter().any(|row| row.kind == RowKind::ExpandedContext)
    }));

    review.press("o");
    review.assert_visible("source-mode-header");
    review.press("o");
    review.assert_hidden("source-mode-header");
}
