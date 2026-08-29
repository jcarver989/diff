use diff_core::{DiffReviewEvent, RepositoryAction, ViewMode, testing::DocumentBuilder};
use diff_gpui::{DiffViewer, DiffViewerEvent, ViewerPane};
use gpui::{Context, Entity, Render, TestAppContext, Window, WindowOptions, div, prelude::*};

struct TestRoot {
    viewer: Entity<DiffViewer>,
    events: Vec<DiffViewerEvent>,
}

impl TestRoot {
    fn new(cx: &mut Context<Self>) -> Self {
        let viewer = cx.new(|_| {
            DiffViewer::new(
                DocumentBuilder::new()
                    .changed("a.rs", "one\ntwo\nthree\n", "ONE\nTWO\nTHREE\n")
                    .changed("b.rs", "old\n", "new\n")
                    .build(),
            )
        });
        cx.subscribe(&viewer, |root, _, event: &DiffViewerEvent, _| {
            root.events.push(event.clone());
        })
        .detach();
        Self {
            viewer,
            events: Vec::new(),
        }
    }
}

impl Render for TestRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.viewer.clone())
    }
}

fn open_viewer(cx: &mut TestAppContext) -> gpui::WindowHandle<TestRoot> {
    cx.update(DiffViewer::bind_keys);
    let window = cx.update(|cx| {
        cx.open_window(WindowOptions::default(), |_, cx| cx.new(TestRoot::new))
            .unwrap()
    });
    cx.run_until_parked();
    window
}

#[gpui::test]
fn browse_shortcuts_navigate_rows_files_and_panes(cx: &mut TestAppContext) {
    let window = open_viewer(cx);
    let viewer = window.read_with(cx, |root, _| root.viewer.clone()).unwrap();
    let first_row = viewer
        .read_with(cx, |viewer, _| viewer.session().selected_row())
        .unwrap();

    cx.simulate_keystrokes(*window, "j");
    let next_row = viewer
        .read_with(cx, |viewer, _| viewer.session().selected_row())
        .unwrap();
    assert!(next_row > first_row);

    cx.simulate_keystrokes(*window, "h j");
    viewer.read_with(cx, |viewer, _| {
        assert_eq!(viewer.pane(), ViewerPane::Files);
        assert_eq!(viewer.selected_file(), Some(1));
    });

    cx.simulate_keystrokes(*window, "tab");
    assert_eq!(
        viewer.read_with(cx, |viewer, _| viewer.pane()),
        ViewerPane::Diff
    );
}

#[gpui::test]
fn comment_shortcuts_add_edit_delete_undo_and_cancel_drafts(cx: &mut TestAppContext) {
    let window = open_viewer(cx);
    let viewer = window.read_with(cx, |root, _| root.viewer.clone()).unwrap();

    cx.simulate_keystrokes(*window, "ctrl-c");
    assert!(viewer.read_with(cx, |viewer, _| viewer.session().draft().is_none()));

    cx.simulate_keystrokes(*window, "c shift-enter");
    viewer.read_with(cx, |viewer, _| {
        assert_eq!(viewer.session().draft().unwrap().body(), "\n");
    });
    cx.simulate_keystrokes(*window, "enter");
    viewer.read_with(cx, |viewer, _| {
        assert!(viewer.session().draft().is_none());
        assert!(viewer.review().is_empty());
    });

    cx.simulate_keystrokes(*window, "c");
    assert!(viewer.read_with(cx, |viewer, _| viewer.session().draft().is_some()));
    cx.simulate_keystrokes(*window, "escape");
    assert!(viewer.read_with(cx, |viewer, _| viewer.session().draft().is_none()));

    viewer.update(cx, |viewer, cx| {
        let anchor = viewer.session().selected_anchor().unwrap();
        viewer.add_comment(anchor, "ONE", "first", cx);
        let anchor = viewer.session().selected_anchor().unwrap();
        viewer.add_comment(anchor, "ONE", "second", cx);
    });
    assert_eq!(viewer.read_with(cx, |viewer, _| viewer.review().len()), 2);

    cx.simulate_keystrokes(*window, "alt-x");
    assert_eq!(viewer.read_with(cx, |viewer, _| viewer.review().len()), 2);

    cx.simulate_keystrokes(*window, "u");
    assert_eq!(viewer.read_with(cx, |viewer, _| viewer.review().len()), 1);

    cx.simulate_keystrokes(*window, "e");
    viewer.read_with(cx, |viewer, _| {
        let draft = viewer.session().draft().unwrap();
        assert_eq!(draft.body(), "first");
        assert!(draft.editing().is_some());
    });
    cx.simulate_keystrokes(*window, "escape x");
    assert!(viewer.read_with(cx, |viewer, _| viewer.review().is_empty()));
}

#[gpui::test]
fn repository_shortcuts_emit_path_bulk_commit_and_discard_actions(cx: &mut TestAppContext) {
    let window = open_viewer(cx);

    cx.simulate_keystrokes(*window, "h space a shift-a d y shift-c s h i p enter");
    window
        .read_with(cx, |root, _| {
            assert!(matches!(
                &root.events[0],
                DiffReviewEvent::RepositoryAction(RepositoryAction::StagePaths(paths))
                    if paths.len() == 1 && paths[0].as_str() == "a.rs"
            ));
            assert!(root.events.contains(&DiffReviewEvent::RepositoryAction(
                RepositoryAction::StageAll
            )));
            assert!(root.events.contains(&DiffReviewEvent::RepositoryAction(
                RepositoryAction::UnstageAll
            )));
            assert!(root.events.iter().any(|event| matches!(
                event,
                DiffReviewEvent::RepositoryAction(RepositoryAction::Discard { path, .. })
                    if path.as_str() == "a.rs"
            )));
            assert!(root.events.contains(&DiffReviewEvent::RepositoryAction(
                RepositoryAction::Commit {
                    message: "ship".to_owned(),
                }
            )));
        })
        .unwrap();
}

#[gpui::test]
fn review_help_layout_and_host_event_shortcuts_are_contextual(cx: &mut TestAppContext) {
    let window = open_viewer(cx);
    let viewer = window.read_with(cx, |root, _| root.viewer.clone()).unwrap();

    assert_eq!(
        viewer.read_with(cx, |viewer, _| viewer.view_mode()),
        ViewMode::Auto
    );
    cx.simulate_keystrokes(*window, "v shift-/");
    viewer.read_with(cx, |viewer, _| {
        assert_eq!(viewer.view_mode(), ViewMode::Unified);
        assert!(viewer.shortcuts_open());
    });

    let selected = viewer
        .read_with(cx, |viewer, _| viewer.session().selected_row())
        .unwrap();
    cx.simulate_keystrokes(*window, "j escape");
    viewer.read_with(cx, |viewer, _| {
        assert_eq!(viewer.session().selected_row(), Some(selected));
        assert!(!viewer.shortcuts_open());
    });

    cx.simulate_keystrokes(*window, "s y escape");
    window
        .read_with(cx, |root, _| {
            assert!(matches!(
                root.events.first(),
                Some(DiffReviewEvent::SubmitReview(_))
            ));
            assert!(matches!(root.events.last(), Some(DiffReviewEvent::Cancel)));
            assert_eq!(
                root.events.len(),
                2,
                "copying an empty review remains a no-op"
            );
        })
        .unwrap();
}
