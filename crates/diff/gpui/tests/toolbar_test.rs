#![cfg(feature = "test-support")]

use clankerdiff_gpui::testing::DiffViewerHarnessBuilder;
use gpui::TestAppContext;

#[gpui::test]
fn the_files_modes_sit_over_the_sidebar_and_follow_the_toggle_when_it_hides(cx: &mut TestAppContext) {
    DiffViewerHarnessBuilder::default().with_lookups().with_window_size(1400.0, 800.0).run(cx, |harness, cx| {
        let column = harness.bounds(cx, "toolbar-files").ok_or("missing files column")?;
        let modes = harness.bounds(cx, "files-mode").ok_or("missing files modes")?;
        assert!(column.left() <= modes.left() && modes.right() <= column.right());
        assert!(harness.bounds(cx, "files-mode-diff-detail").is_some(), "the diff mode counts its changes");
        assert!(harness.bounds(cx, "files-mode-browse-detail").is_none());

        harness.click(cx, "diff-sidebar-toggle")?;
        assert!(harness.bounds(cx, "toolbar-files").is_none());
        let toggle = harness.bounds(cx, "diff-sidebar-toggle").ok_or("missing sidebar toggle")?;
        let modes = harness.bounds(cx, "files-mode").ok_or("missing files modes")?;
        let back = harness.bounds(cx, "navigate-back").ok_or("missing back")?;
        assert!(toggle.right() <= modes.left() && modes.right() < back.left());
        Ok(())
    });
}
