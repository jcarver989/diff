use diff_core::testing::DocumentBuilder;
use diff_gpui::testing::DiffViewerHarnessBuilder;
use gpui::{TestAppContext, px};

fn viewer_builder() -> DiffViewerHarnessBuilder {
    DiffViewerHarnessBuilder {
        snapshot: DocumentBuilder::new()
            .changed_with_hunk_window(
                "src/main.rs",
                "one\ntwo\nthree\nfour\nfive\n",
                "one\ntwo\nTHREE\nfour\nfive\n",
                2..=4,
            )
            .changed("README.md", "old\n", "new\n")
            .build_fixture()
            .snapshot(),
        ..DiffViewerHarnessBuilder::default()
    }
}

#[gpui::test]
fn renders_the_viewer_sidebar_and_diff_pane(cx: &mut TestAppContext) {
    let harness = viewer_builder().build(cx);

    let root = harness
        .bounds(cx, "diff-viewer")
        .expect("viewer is painted");
    let content = harness
        .bounds(cx, "diff-viewer-content")
        .expect("viewer content is painted");
    let sidebar = harness
        .bounds(cx, "diff-sidebar")
        .expect("sidebar is painted");
    let files = harness
        .bounds(cx, "diff-files")
        .expect("file tree is painted");
    let diff = harness
        .bounds(cx, "diff-pane")
        .expect("diff pane is painted");

    assert!(root.size.width > px(0.0));
    assert!(
        root.size.height > content.size.height,
        "review bar uses remaining height"
    );
    assert_eq!(sidebar.origin.x, content.origin.x);
    assert_eq!(files.origin.x, sidebar.origin.x);
    assert!(diff.origin.x > sidebar.origin.x);
    assert!(diff.size.width > sidebar.size.width);
}

#[gpui::test]
fn comment_shortcut_renders_the_real_editor(cx: &mut TestAppContext) {
    let harness = viewer_builder().build(cx);
    assert!(harness.bounds(cx, "comment-input").is_none());

    harness.simulate_keystrokes(cx, "c");

    let editor = harness
        .bounds(cx, "comment-input")
        .expect("comment editor is painted after the shortcut");
    let diff = harness
        .bounds(cx, "diff-pane")
        .expect("diff pane is painted");
    assert!(editor.size.width > px(0.0));
    assert!(editor.size.height >= px(96.0));
    assert!(editor.origin.x >= diff.origin.x);
}
