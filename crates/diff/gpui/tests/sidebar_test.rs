#![cfg(feature = "test-support")]

use clankerdiff_core::{DiffReviewCommand, FocusPane, testing::DocumentBuilder};
use clankerdiff_gpui::testing::{DiffViewerHarness, DiffViewerHarnessBuilder};
use clankerdiff_gpui_ui::testing::selector;
use gpui::{ScrollDelta, TestAppContext, VisualTestContext, point, px};

const FILES: usize = 300;

#[gpui::test]
fn a_long_file_tree_renders_only_the_rows_in_view_and_scrolls_the_selection_into_it(cx: &mut TestAppContext) {
    long_tree_builder().run(cx, |harness, cx| {
        assert!(file_row_shown(harness, cx, 0), "the first file is in view");
        let shown = rows_shown(harness, cx);
        assert!(shown < FILES / 4, "rendered {shown} of {FILES} rows");

        assert!(harness.dispatch_command(cx, DiffReviewCommand::Focus(FocusPane::Files))?);
        assert!(harness.dispatch_command(cx, DiffReviewCommand::Last)?);
        assert_eq!(harness.selected_file(cx), Some(FILES - 1));
        let files = harness.bounds(cx, "diff-files").ok_or("missing file tree")?;
        let row = harness.bounds(cx, checkbox(FILES - 1)).ok_or("the selected file is scrolled into view")?;
        assert!(files.top() <= row.top() && row.bottom() <= files.bottom());
        assert!(!file_row_shown(harness, cx, 0), "the first file scrolled out of view");
        Ok(())
    });
}

#[gpui::test]
fn paging_the_file_tree_moves_the_selection_by_the_rows_in_view(cx: &mut TestAppContext) {
    long_tree_builder().run(cx, |harness, cx| {
        assert!(harness.dispatch_command(cx, DiffReviewCommand::Focus(FocusPane::Files))?);
        assert!(harness.dispatch_command(cx, DiffReviewCommand::First)?);
        let mut position = 0;
        for _ in 0..2 {
            position += rows_shown(harness, cx);
            assert!(harness.dispatch_command(cx, DiffReviewCommand::Page(1))?);
            assert_eq!(harness.selected_file(cx), Some(position - 1));
            assert!(file_row_shown(harness, cx, position - 1), "the paged-to file is in view");
        }
        Ok(())
    });
}

#[gpui::test]
fn paging_a_part_scrolled_file_tree_counts_its_partly_shown_rows(cx: &mut TestAppContext) {
    long_tree_builder().run(cx, |harness, cx| {
        assert!(harness.dispatch_command(cx, DiffReviewCommand::Focus(FocusPane::Files))?);
        assert!(harness.dispatch_command(cx, DiffReviewCommand::First)?);
        harness.scroll(cx, "diff-files", ScrollDelta::Pixels(point(px(0.0), px(-7.0))))?;
        let rows = rows_shown(harness, cx);

        assert!(harness.dispatch_command(cx, DiffReviewCommand::Page(1))?);

        assert_eq!(harness.selected_file(cx), Some(rows - 1));
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

fn rows_shown(harness: &DiffViewerHarness, cx: &mut TestAppContext) -> usize {
    harness.draw(cx);
    let mut visual = VisualTestContext::from_window(harness.window(), cx);
    let directory = usize::from(visual.debug_bounds("diff-directory-checkbox:src").is_some());
    directory + (0..FILES).filter(|&file| visual.debug_bounds(checkbox(file)).is_some()).count()
}

fn file_row_shown(harness: &DiffViewerHarness, cx: &mut TestAppContext, file: usize) -> bool {
    harness.bounds(cx, checkbox(file)).is_some()
}

fn checkbox(file: usize) -> &'static str {
    selector(format!("diff-file-checkbox:{file}"))
}
