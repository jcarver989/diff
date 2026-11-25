mod common;

use clankerdiff_core::{
    DiffDocument, DiffReviewCommand as C, DiffSide, RowKind, ViewMode, parse_detached_git_diff,
    testing::DocumentBuilder,
};
use clankerdiff_rouge::{ReviewController, ReviewHandle, ReviewOutput, ViewerPane};
use clankerdiff_ui_core::ReviewTheme;
use common::ReviewHarness;
use rouge_core::{
    Point, Rect, Size,
    testing::{TestResult, TestUi},
};
use std::{error::Error, fmt::Write, io, sync::Arc};

#[test]
fn split_and_unified_render_the_same_patch_and_target_the_correct_side() -> TestResult {
    let doc = DocumentBuilder::new().changed("src/lib.rs", "let old = 1;\n", "let new = 2;\n").build();
    let mut review = ReviewHarness::new(doc);
    assert!(review.handle.handle_command(C::SetViewMode(ViewMode::Split)));
    let row = first_code(&review.handle)?;
    let old = review.ui.center_of(&format!("diff-cell-{row}-Old"))?;
    let new = review.ui.center_of(&format!("diff-cell-{row}-New"))?;
    assert!(old.x < new.x);
    review.ui.click(old);
    assert_eq!(review.handle.selection().get().side, DiffSide::Old);
    review.ui.click(new);
    assert_eq!(review.handle.selection().get().side, DiffSide::New);
    assert!(review.handle.handle_command(C::SetViewMode(ViewMode::Unified)));
    review.ui.advance(0.0);
    review.assert_selected_line("src/lib.rs", 1)?;
    assert!(review.outputs().contains(&ReviewOutput::ViewModeChanged(ViewMode::Unified)));
    Ok(())
}

#[test]
fn markdown_files_toggle_between_their_diff_and_a_rendered_preview() -> TestResult {
    let mut review = ReviewHarness::new(markdown_change().changed("src/lib.rs", "old\n", "new\n").build());
    review.assert_visible("markdown-toggle-0");
    review.assert_hidden("markdown-toggle-1");

    review.click("preview-markdown-0")?;
    review.assert_visible("markdown-preview-0");

    review.click("show-diff-0")?;
    review.assert_hidden("markdown-preview-0");
    review.select_file("README.md")?;
    review.press("m");
    review.assert_visible("markdown-preview-0");
    Ok(())
}

#[test]
fn markdown_preview_toggles_from_full_file_source_and_back() -> TestResult {
    let mut review = ReviewHarness::new(markdown_change().changed("src/lib.rs", "old\n", "new\n").build());
    review.select_file("README.md")?;
    review.press("o");
    review.assert_selected_line("README.md", 3)?;
    review.assert_visible("show-source-0");

    review.press("m");
    review.assert_visible("markdown-preview-0");
    review.ui.click_text("the guide")?;
    assert!(review.outputs().contains(&ReviewOutput::OpenLink("https://example.com/guide".to_owned())));
    review.click("show-source-0")?;
    review.assert_hidden("markdown-preview-0");
    review.assert_selected_line("README.md", 3)?;

    review.press("o");
    review.assert_hidden("source-mode-header");
    review.assert_hidden("markdown-preview-0");
    review.assert_visible("show-diff-0");
    Ok(())
}

#[test]
fn same_count_replacement_retargets_mounted_rows_and_sidebar() -> TestResult {
    let doc = DocumentBuilder::new().changed("src/old.rs", "", "fn before() {}\n").build();
    let mut review = ReviewHarness::new(doc);
    let row = first_code(&review.handle)?;
    let point = review.ui.center_of(&format!("diff-cell-{row}-New"))?;
    let next = DocumentBuilder::new().changed("src/new.rs", "", "fn after() {}\n").build();
    review.handle.set_document(next);
    review.ui.advance(0.0);
    assert_eq!(first_code(&review.handle)?, row);
    review.ui.click(point);
    review.assert_selected_line("src/new.rs", 1)?;
    review.ui.hover_on(&format!("add-comment-{row}-New"))?;
    review.ui.advance(0.0);
    review.click(&format!("add-comment-{row}-New"))?;
    review.ui.type_text("after refresh");
    review.press("enter");
    assert!(review.handle.with(|s| s.session().review().comments().any(|c| c.anchor.path.as_str() == "src/new.rs")));
    review.click("toggle-sidebar")?;
    assert!(review.handle.status().get().sidebar_collapsed);
    review.click("toggle-sidebar")?;
    review.click("diff-file-0")?;
    assert_eq!(review.handle.selection().get().pane, ViewerPane::Files);
    Ok(())
}

#[test]
fn virtual_list_keeps_large_documents_bounded_and_reveals_last_row() -> TestResult {
    let mut patch = String::new();
    for file in 0..10 {
        let name = format!("large-{file}.txt");
        writeln!(
            patch,
            "diff --git a/{name} b/{name}\nnew file mode 100644\n--- /dev/null\n+++ b/{name}\n@@ -0,0 +1,10000 @@"
        )?;
        for n in 0..10_000 {
            writeln!(patch, "+line {n}")?;
        }
    }
    let doc = Arc::new(DiffDocument { files: parse_detached_git_diff(patch.as_bytes())?, ..DiffDocument::default() });
    let mut review = ReviewHarness::new(doc);
    review.ui.advance(0.0);
    assert!(review.ui.stats().elements < 2_500);
    assert!(review.handle.handle_command(C::Last));
    let row = review.handle.selection().get().row.ok_or_else(|| io::Error::other("no selected last row"))?;
    let point = review.ui.center_of(&format!("diff-cell-{row}-New"))?;
    assert!(point.y > 0.0 && point.y < 800.0);
    assert!(review.ui.stats().elements < 2_500);
    assert!(review.handle.handle_command(C::First));
    let row = first_code(&review.handle)?;
    assert!(review.ui.center_of(&format!("diff-cell-{row}-New"))?.y < 800.0);
    Ok(())
}

#[test]
fn sticky_file_tracks_scroll_and_preserves_location_across_scale_and_refresh() -> TestResult {
    let lines = "line content\n".repeat(100);
    let doc = || DocumentBuilder::new().changed("a.txt", "", &lines).changed("b.txt", "", &lines).build();
    let mut review = ReviewHarness::new(doc());
    let point = review.ui.center_of("diff-pane")?;
    review.ui.pointer_moved(point);
    review.ui.scroll(Point::new(0.0, -560.0));
    review.ui.advance(0.0);
    let header = review.ui.center_of("sticky-file-header")?;
    assert!(header.y > 10.0 && header.y < 120.0);
    let row = first_code(&review.handle)? + 29;
    let before = review.ui.center_of(&format!("diff-cell-{row}-New"))?;
    review.handle.set_document(doc());
    let after = review.ui.center_of(&format!("diff-cell-{row}-New"))?;
    assert!((after.y - before.y).abs() < 1.0, "{before:?} vs {after:?}");
    review.ui.set_size(Size::new(1400.0, 900.0));
    review.ui.advance(0.0);
    assert!(review.ui.center_of("sticky-file-header")?.y < 120.0);
    Ok(())
}

#[test]
fn keyboard_reveal_clears_the_sticky_header_and_next_file_pushes_it_off() -> TestResult {
    let lines = "line content\n".repeat(100);
    let doc = DocumentBuilder::new().changed("a.txt", "", &lines).changed("b.txt", "", &lines).build();
    let mut review = ReviewHarness::new(doc);
    let row = first_code(&review.handle)? + 20;
    let cell = format!("diff-cell-{row}-New");
    let pane = review.ui.bounds_of("diff-pane").ok_or_else(|| io::Error::other("no diff pane"))?;
    let before = review.ui.center_of(&cell)?;
    review.ui.pointer_moved(pane.center());
    review.ui.scroll(Point::new(0.0, pane.origin.y + 10.0 - before.y));
    review.ui.advance(0.0);
    let sticky = || io::Error::other("no sticky header");
    let header = review.ui.bounds_of("sticky-file-header").ok_or_else(sticky)?;
    let covered = review.ui.center_of(&cell)?;
    assert!(
        covered.y >= header.origin.y && covered.y < header.origin.y + header.size.height,
        "{covered:?} not in {header:?}"
    );
    assert!(review.handle.handle_command(C::SelectRow(row + 1)));
    review.press("up");
    assert_eq!(review.handle.selection().get().row, Some(row));
    let revealed = review.ui.bounds_of(&cell).ok_or_else(|| io::Error::other("no revealed row"))?;
    let header = review.ui.bounds_of("sticky-file-header").ok_or_else(sticky)?;
    assert!(revealed.origin.y >= header.origin.y + header.size.height - 0.1);

    assert!(review.handle.handle_command(C::SelectFile(1)));
    review.ui.advance(0.0);
    let incoming = review.ui.bounds_of("diff-file-header-1").ok_or_else(|| io::Error::other("no next file header"))?;
    review.ui.scroll(Point::new(0.0, pane.origin.y + 16.0 - incoming.origin.y));
    review.ui.advance(0.0);
    let outgoing = review.ui.bounds_of("sticky-file-header").ok_or_else(sticky)?;
    let incoming = review.ui.bounds_of("diff-file-header-1").ok_or_else(|| io::Error::other("no next file header"))?;
    let pane = review.ui.bounds_of("diff-pane").ok_or_else(|| io::Error::other("no diff pane"))?;
    assert!(outgoing.origin.y < pane.origin.y && incoming.origin.y > pane.origin.y);
    assert!((outgoing.origin.y + outgoing.size.height - incoming.origin.y).abs() < 0.1);
    Ok(())
}

#[test]
fn long_code_scrolls_under_fixed_gutters_when_wrapping_is_off() -> TestResult {
    let doc = DocumentBuilder::new().changed("long.txt", "", &format!("{}\n", "long line ".repeat(250))).build();
    let mut review = ReviewHarness::new(doc);
    assert!(review.handle.handle_command(C::ToggleWrap));
    review.ui.advance(0.0);
    let row = first_code(&review.handle)?;
    let cell = review.ui.center_of(&format!("diff-cell-{row}-New"))?;
    let code = format!("code-{row}-New");
    let before = review.ui.center_of(&code)?;
    review.ui.pointer_moved(cell);
    review.ui.scroll(Point::new(-220.0, 0.0));
    let after = review.ui.center_of(&code)?;
    assert!(after.x < before.x - 100.0, "{before:?} -> {after:?}");
    assert_eq!(review.ui.center_of(&format!("diff-cell-{row}-New"))?, cell);
    Ok(())
}

#[test]
fn long_code_wraps_inside_its_cell_and_marks_each_continuation_line() -> TestResult {
    let doc = DocumentBuilder::new().changed("long.txt", "", &format!("    {}\n", "wrapped words ".repeat(60))).build();
    let mut review = ReviewHarness::new(doc);
    let row = first_code(&review.handle)?;
    let cell = bounds(&mut review.ui, &format!("diff-cell-{row}-New"))?;
    let code = bounds(&mut review.ui, &format!("code-{row}-New"))?;
    let marks = review.ui.bounds_of_all(&format!("code-{row}-New:continuation"));
    assert!(marks.len() >= 3, "{} continuation lines", marks.len());
    assert!(code.max().x <= cell.max().x, "{code:?} inside {cell:?}");
    assert!(cell.size.height >= code.size.height, "{cell:?} fits {code:?}");
    assert!(marks.iter().all(|mark| mark.max().x <= code.origin.x && mark.origin.x >= cell.origin.x));
    assert!(marks.windows(2).all(|pair| pair[1].origin.y > pair[0].origin.y));

    review.ui.pointer_moved(cell.center());
    review.ui.scroll(Point::new(-220.0, 0.0));
    review.ui.advance(0.0);
    assert_eq!(
        bounds(&mut review.ui, &format!("code-{row}-New"))?.origin,
        code.origin,
        "wrapped rows don't scroll sideways"
    );
    Ok(())
}

#[test]
fn split_rows_grow_to_the_taller_side_and_any_wrapped_line_selects_the_cell() -> TestResult {
    let long = format!("let long = \"{}\";\n", "text ".repeat(80));
    let doc = DocumentBuilder::new().changed("src/lib.rs", "let short = 1;\n", &long).build();
    let mut review = ReviewHarness::new(doc);
    assert!(review.handle.handle_command(C::SetViewMode(ViewMode::Split)));
    review.ui.advance(0.0);
    let row = first_code(&review.handle)?;
    let old = bounds(&mut review.ui, &format!("diff-cell-{row}-Old"))?;
    let new = bounds(&mut review.ui, &format!("diff-cell-{row}-New"))?;
    let short = bounds(&mut review.ui, &format!("code-{row}-Old"))?;
    let wrapped = bounds(&mut review.ui, &format!("code-{row}-New"))?;
    assert_eq!(old.size.height, new.size.height);
    assert!(wrapped.size.height > short.size.height * 2.5, "{short:?} {wrapped:?}");
    assert!(new.size.height >= wrapped.size.height);

    review.ui.click(Point::new(old.center().x, old.max().y - 4.0));
    assert_eq!((review.handle.selection().get().row, review.handle.selection().get().side), (Some(row), DiffSide::Old));
    review.ui.click(Point::new(wrapped.center().x, wrapped.max().y - 4.0));
    assert_eq!((review.handle.selection().get().row, review.handle.selection().get().side), (Some(row), DiffSide::New));
    Ok(())
}

#[test]
fn toggling_wrap_restores_single_line_rows_and_reports_the_preference() -> TestResult {
    let doc = DocumentBuilder::new().changed("long.txt", "", &format!("{}\n", "long line ".repeat(250))).build();
    let mut review = ReviewHarness::new(doc);
    let row = first_code(&review.handle)?;
    let wrapped = bounds(&mut review.ui, &format!("diff-row-{row}"))?;
    review.press("w");
    let single = bounds(&mut review.ui, &format!("diff-row-{row}"))?;
    assert!(!review.handle.status().get().wrap);
    assert!(single.size.height * 3.0 < wrapped.size.height, "{single:?} {wrapped:?}");
    assert_eq!(review.ui.bounds_of_all(&format!("code-{row}-New:continuation")), Vec::<Rect>::new());
    assert!(review.outputs().contains(&ReviewOutput::WrapChanged(false)));

    review.click("toggle-wrap")?;
    review.ui.advance(0.0);
    assert!(review.handle.status().get().wrap);
    assert_eq!(bounds(&mut review.ui, &format!("diff-row-{row}"))?.size, wrapped.size);
    assert!(review.outputs().contains(&ReviewOutput::WrapChanged(true)));
    Ok(())
}

#[test]
fn overlong_lines_show_a_bounded_prefix() -> TestResult {
    let doc = DocumentBuilder::new().changed("min.js", "", &format!("{}\n", "x".repeat(200_000))).build();
    let mut review = ReviewHarness::new(doc);
    let row = first_code(&review.handle)?;
    let cell = bounds(&mut review.ui, &format!("diff-cell-{row}-New"))?;
    let code = bounds(&mut review.ui, &format!("code-{row}-New"))?;
    let lines = review.ui.bounds_of_all(&format!("code-{row}-New:continuation")).len() + 1;
    assert!(200_000 / lines > 1_000, "only a prefix wraps: {lines} lines");
    assert!(cell.size.height >= code.size.height);
    Ok(())
}

#[test]
fn scrolling_wrapped_rows_only_does_work_for_rows_that_mount() -> TestResult {
    let mut patch = String::new();
    writeln!(
        patch,
        "diff --git a/wide.rs b/wide.rs\nnew file mode 100644\n--- /dev/null\n+++ b/wide.rs\n@@ -0,0 +1,2000 @@"
    )?;
    for n in 0..2000 {
        writeln!(patch, "+    let value_{n} = {};", "compute(alpha, beta, gamma) + ".repeat(1 + n % 7))?;
    }
    let doc = Arc::new(DiffDocument { files: parse_detached_git_diff(patch.as_bytes())?, ..DiffDocument::default() });
    let mut review = ReviewHarness::new(doc);
    let pane = review.ui.bounds_of("diff-pane").ok_or_else(|| io::Error::other("no diff pane"))?;
    review.ui.pointer_moved(Point::new(pane.max().x - 30.0, pane.center().y));
    let mut quiet = 0;
    for _ in 0..120 {
        let before = review.ui.stats();
        review.ui.scroll(Point::new(0.0, -60.0));
        review.ui.advance(0.016);
        let frame = review.ui.stats() - before;
        assert!(frame.relayouts <= 1, "a wrapped mount needs at most one relayout: {frame:?}");
        if frame.elements_created + frame.elements_removed == 0 {
            quiet += 1;
            assert_eq!((frame.layouts, frame.layer_paints, frame.text.shapes), (0, 0, 0), "{frame:?}");
        }
    }
    assert!(quiet > 60, "most scroll frames mount nothing: {quiet}");
    Ok(())
}

#[test]
fn scrolling_a_large_diff_only_does_work_for_rows_that_mount() -> TestResult {
    let mut patch = String::new();
    for file in 0..6 {
        let name = format!("src/file_{file}.rs");
        writeln!(
            patch,
            "diff --git a/{name} b/{name}\nnew file mode 100644\n--- /dev/null\n+++ b/{name}\n@@ -0,0 +1,1500 @@"
        )?;
        for n in 0..1500 {
            writeln!(patch, "+    let value_{n} = compute({n}, \"text\") + other_{n};")?;
        }
    }
    let doc = Arc::new(DiffDocument { files: parse_detached_git_diff(patch.as_bytes())?, ..DiffDocument::default() });
    let mut review = ReviewHarness::new(doc);
    assert!(
        review.ui.wait_until(|_| review.handle.with(ReviewController::symbols_ready)),
        "symbols never became ready"
    );
    let pane = review.ui.bounds_of("diff-pane").ok_or_else(|| io::Error::other("no diff pane"))?;
    review.ui.pointer_moved(Point::new(pane.max().x - 30.0, pane.center().y));
    let (step, frames) = (60_u16, 120_u16);
    for _ in 0..10 {
        review.ui.scroll(Point::new(0.0, -f32::from(step)));
        review.ui.advance(0.016);
    }
    let start = review.ui.stats();
    let mut quiet = 0;
    for _ in 0..frames {
        let before = review.ui.stats();
        review.ui.scroll(Point::new(0.0, -f32::from(step)));
        review.ui.advance(0.016);
        let frame = review.ui.stats() - before;
        assert!(frame.relayouts <= 1, "a mount needs at most one relayout: {frame:?}");
        if frame.elements_created + frame.elements_removed > 0 {
            continue;
        }
        quiet += 1;
        let work = (frame.layouts, frame.layer_paints, frame.text.shapes, frame.reactive.nodes_created);
        assert_eq!(work, (0, 0, 0, 0), "{frame:?}");
        assert!(frame.reactive.effects_run + frame.reactive.memos_run <= 12, "{frame:?}");
    }
    assert!(quiet > frames / 2, "most scroll frames mount nothing: {quiet}");
    let rows = u64::from(frames) * u64::from(step) / 22;
    let total = review.ui.stats() - start;
    assert!(total.elements_created <= 8 * rows, "{} elements", total.elements_created);
    assert!(total.reactive.nodes_created <= 20 * rows, "{} reactive nodes", total.reactive.nodes_created);
    Ok(())
}

#[test]
fn moving_the_selection_only_repaints_the_rows_it_touches() -> TestResult {
    let mut review = ReviewHarness::new(large_document()?);
    assert!(
        review.ui.wait_until(|_| review.handle.with(ReviewController::symbols_ready)),
        "symbols never became ready"
    );
    assert!(review.handle.handle_command(C::SelectRow(first_code(&review.handle)?)));
    review.ui.advance(0.016);
    for _ in 0..5 {
        let before = review.ui.stats();
        assert!(review.handle.handle_command(C::MoveSelection(1)));
        review.ui.advance(0.016);
        let frame = review.ui.stats() - before;
        assert_eq!((frame.layouts, frame.elements_created, frame.elements_removed), (0, 0, 0), "{frame:?}");
        assert!(frame.layer_paints <= 4, "{frame:?}");
        assert!(frame.reactive.effects_run <= 16, "{frame:?}");
    }
    Ok(())
}

#[test]
fn switching_the_theme_repaints_without_layout_or_rebuilding() -> TestResult {
    let mut review = ReviewHarness::new(large_document()?);
    assert!(
        review.ui.wait_until(|_| review.handle.with(ReviewController::symbols_ready)),
        "symbols never became ready"
    );
    let theme = ReviewTheme::builtin("github-light")?;
    let before = review.ui.stats();
    review.handle.set_theme(theme);
    review.ui.advance(0.016);
    let frame = review.ui.stats() - before;
    assert_eq!((frame.layouts, frame.elements_created, frame.elements_removed), (0, 0, 0), "{frame:?}");
    assert!(frame.layer_paints > 0, "{frame:?}");
    Ok(())
}

fn markdown_change() -> DocumentBuilder {
    DocumentBuilder::new().changed_with_hunk_window(
        "README.md",
        "# Guide\n\nOld text\n",
        "# Guide\n\nRead [the guide](https://example.com/guide).\n",
        3..=3,
    )
}

fn large_document() -> Result<Arc<DiffDocument>, Box<dyn Error>> {
    let mut patch = String::new();
    for file in 0..3 {
        let name = format!("src/file_{file}.rs");
        writeln!(
            patch,
            "diff --git a/{name} b/{name}\nnew file mode 100644\n--- /dev/null\n+++ b/{name}\n@@ -0,0 +1,300 @@"
        )?;
        for n in 0..300 {
            writeln!(patch, "+    let value_{n} = compute({n});")?;
        }
    }
    Ok(Arc::new(DiffDocument { files: parse_detached_git_diff(patch.as_bytes())?, ..DiffDocument::default() }))
}

fn first_code(handle: &ReviewHandle) -> Result<usize, io::Error> {
    handle
        .with(|s| {
            let p = s.session().presentation();
            p.rows(0..p.row_count()).iter().position(|row| row.kind == RowKind::Code)
        })
        .ok_or_else(|| io::Error::other("missing code row"))
}

fn bounds(ui: &mut TestUi, name: &str) -> Result<Rect, io::Error> {
    ui.bounds_of(name).ok_or_else(|| io::Error::other(format!("no bounds for {name}")))
}
