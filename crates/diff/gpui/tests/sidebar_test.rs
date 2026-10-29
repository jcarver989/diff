#![cfg(feature = "test-support")]

use clankerdiff_core::{DiffReviewCommand, FocusPane, testing::DocumentBuilder};
use clankerdiff_gpui::testing::{DiffViewerHarness, DiffViewerHarnessBuilder};
use clankerdiff_gpui_ui::testing::selector;
use gpui::{Bounds, Pixels, TestAppContext, VisualTestContext};

const FILES: usize = 300;

#[gpui::test]
fn a_long_file_tree_renders_only_the_rows_in_view_and_scrolls_the_selection_into_it(cx: &mut TestAppContext) {
    long_tree_builder().run(cx, |harness, cx| {
        assert!(file_row_shown(harness, cx, 0), "the first file is in view");
        let shown = file_rows_shown(harness, cx);
        assert!(shown < FILES / 4, "rendered {shown} of {FILES} file rows");

        assert!(harness.dispatch_command(cx, DiffReviewCommand::Focus(FocusPane::Files))?);
        assert!(harness.dispatch_command(cx, DiffReviewCommand::Last)?);
        assert_eq!(selected_file(harness, cx), Some(FILES - 1));
        let files = harness.bounds(cx, "diff-files").ok_or("missing file tree")?;
        let row = file_row(harness, cx, FILES - 1).ok_or("the selected file is scrolled into view")?;
        assert!(files.top() <= row.top() && row.bottom() <= files.bottom());
        assert!(!file_row_shown(harness, cx, 0), "the first file scrolled out of view");
        Ok(())
    });
}

#[gpui::test]
fn paging_the_file_tree_moves_a_screenful_and_keeps_the_selection_in_view(cx: &mut TestAppContext) {
    long_tree_builder().run(cx, |harness, cx| {
        assert!(harness.dispatch_command(cx, DiffReviewCommand::Focus(FocusPane::Files))?);
        assert!(harness.dispatch_command(cx, DiffReviewCommand::First)?);
        let shown = file_rows_shown(harness, cx);

        assert!(harness.dispatch_command(cx, DiffReviewCommand::Page(1))?);
        let first_page = selected_file(harness, cx).ok_or("a file is selected")?;
        assert!(first_page + 3 >= shown && first_page <= shown, "paged to file {first_page} with {shown} rows shown");
        assert!(file_row_shown(harness, cx, first_page));

        assert!(harness.dispatch_command(cx, DiffReviewCommand::Page(1))?);
        let second_page = selected_file(harness, cx).ok_or("a file is selected")?;
        assert!(second_page + 3 >= first_page + shown, "paged from file {first_page} to {second_page}");
        assert!(file_row_shown(harness, cx, second_page));
        Ok(())
    });
}

#[gpui::test]
fn scrolling_the_file_tree_moves_its_top_row_by_the_given_lines(cx: &mut TestAppContext) {
    long_tree_builder().run(cx, |harness, cx| {
        assert!(harness.dispatch_command(cx, DiffReviewCommand::Scroll { pane: FocusPane::Files, lines: 3 })?);
        assert!(!file_row_shown(harness, cx, 1), "the second file scrolled out of view");
        assert!(file_row_shown(harness, cx, 2), "the third file is the top row");

        assert!(harness.dispatch_command(cx, DiffReviewCommand::Scroll { pane: FocusPane::Files, lines: -3 })?);
        assert!(file_row_shown(harness, cx, 0), "scrolling back up shows the first file");
        Ok(())
    });
}

fn long_tree_builder() -> DiffViewerHarnessBuilder {
    let document = (0..FILES)
        .fold(DocumentBuilder::new(), |builder, file| builder.generated(&format!("src/file_{file:03}.rs"), 2))
        .build();
    DiffViewerHarnessBuilder { document, ..Default::default() }.with_window_size(1200.0, 600.0)
}

fn selected_file(harness: &DiffViewerHarness, cx: &TestAppContext) -> Option<usize> {
    harness.read(cx, |viewer, _| viewer.selected_file())
}

fn file_rows_shown(harness: &DiffViewerHarness, cx: &mut TestAppContext) -> usize {
    harness.draw(cx);
    let mut visual = VisualTestContext::from_window(harness.window(), cx);
    (0..FILES).filter(|&file| visual.debug_bounds(checkbox(file)).is_some()).count()
}

fn file_row_shown(harness: &DiffViewerHarness, cx: &mut TestAppContext, file: usize) -> bool {
    file_row(harness, cx, file).is_some()
}

fn file_row(harness: &DiffViewerHarness, cx: &mut TestAppContext, file: usize) -> Option<Bounds<Pixels>> {
    harness.bounds(cx, checkbox(file))
}

fn checkbox(file: usize) -> &'static str {
    selector(format!("diff-file-checkbox:{file}"))
}
