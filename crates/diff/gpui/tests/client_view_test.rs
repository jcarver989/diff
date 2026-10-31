#![cfg(feature = "test-support")]

use clankerdiff_client::{ClientState, ConnectionState};
use clankerdiff_core::{ReviewCapabilities, testing::DocumentBuilder};
use clankerdiff_gpui::ui::testing::ViewHarness;
use clankerdiff_gpui::{DiffClientEvent, DiffClientView, DiffViewer};
use clankerdiff_ui_core::ReviewTheme;
use gpui::{AppContext, TestAppContext, WindowOptions};

#[gpui::test]
fn the_host_hears_how_many_comments_the_review_holds(cx: &mut TestAppContext) {
    let harness = connected(cx);

    harness.simulate_keystrokes(cx, "c h i enter");
    assert_eq!(comments(&harness, cx).last(), Some(&1), "a saved comment is counted");

    harness.update(cx, DiffClientView::clear_review);
    harness.draw(cx);
    assert_eq!(comments(&harness, cx).last(), Some(&0), "a submitted review is emptied");
}

#[gpui::test]
fn moving_through_the_diff_says_nothing_about_the_review(cx: &mut TestAppContext) {
    let harness = connected(cx);

    harness.simulate_keystrokes(cx, "j j k");

    assert_eq!(comments(&harness, cx), Vec::<usize>::new());
}

fn connected(cx: &mut TestAppContext) -> ViewHarness<DiffClientView, DiffClientEvent> {
    cx.update(DiffViewer::bind_keys);
    let state = ClientState {
        snapshot: Some(DocumentBuilder::new().changed("src/lib.rs", "old\n", "new\n").build()),
        connection: ConnectionState::Connected,
        capabilities: ReviewCapabilities::default(),
        error: None,
        files: None,
    };
    let harness = ViewHarness::open(cx, WindowOptions::default(), |_, cx| {
        cx.new(|cx| {
            let mut view = DiffClientView::new(ReviewTheme::default());
            view.set_state(&state, cx);
            view
        })
    });
    harness.draw(cx);
    harness
}

fn comments(harness: &ViewHarness<DiffClientView, DiffClientEvent>, cx: &TestAppContext) -> Vec<usize> {
    harness
        .events(cx)
        .into_iter()
        .filter_map(|event| match event {
            DiffClientEvent::ReviewChanged { comments } => Some(comments),
            _ => None,
        })
        .collect()
}
