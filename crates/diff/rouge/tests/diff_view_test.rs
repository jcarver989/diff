mod common;

use clankerdiff_core::{
    DiffDocument, DiffReviewCommand as C, DiffSide, RowKind, ViewMode, parse_detached_git_diff,
    testing::{DocumentBuilder, location},
};
use clankerdiff_rouge::{ReviewController, ReviewOutput, ViewerPane};
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{Key, Point, Size, testing::TestResult};
use std::{fmt::Write, io, sync::Arc};

#[test]
fn split_and_unified_render_the_same_patch_and_target_the_correct_side() -> TestResult {
    let doc = DocumentBuilder::new().changed("src/lib.rs", "let old = 1;\n", "let new = 2;\n").build();
    common::run(doc, |ui, handle, outputs| {
        assert!(handle.handle_command(C::SetViewMode(ViewMode::Split)));
        let row = first_code(&handle)?;
        let old = ui.center_of(&format!("diff-cell-{row}-Old"))?;
        let new = ui.center_of(&format!("diff-cell-{row}-New"))?;
        assert!(old.x < new.x);
        ui.click(old);
        assert_eq!(handle.selection().get().side, DiffSide::Old);
        ui.click(new);
        assert_eq!(handle.selection().get().side, DiffSide::New);
        assert!(handle.handle_command(C::SetViewMode(ViewMode::Unified)));
        ui.advance(0.0);
        let location = location("src/lib.rs", 1)?;
        assert_eq!(handle.with(|s| s.session().selected_source_line()), Some(location));
        assert!(outputs.borrow().contains(&ReviewOutput::ViewModeChanged(ViewMode::Unified)));
        Ok(())
    })
}

#[test]
fn same_count_replacement_retargets_mounted_rows_and_sidebar() -> TestResult {
    let doc = DocumentBuilder::new().changed("src/old.rs", "", "fn before() {}\n").build();
    common::run(doc, |ui, handle, _| {
        let row = first_code(&handle)?;
        let point = ui.center_of(&format!("diff-cell-{row}-New"))?;
        let next = DocumentBuilder::new().changed("src/new.rs", "", "fn after() {}\n").build();
        handle.set_document(next);
        ui.advance(0.0);
        assert_eq!(first_code(&handle)?, row);
        ui.click(point);
        assert_eq!(handle.with(|s| s.session().selected_source_line()), Some(location("src/new.rs", 1)?));
        ui.hover_on(&format!("add-comment-{row}-New"))?;
        ui.advance(0.0);
        ui.click_on(&format!("add-comment-{row}-New"))?;
        ui.type_text("after refresh");
        ui.press(Key::Enter);
        assert!(handle.with(|s| s.session().review().comments().any(|c| c.anchor.path.as_str() == "src/new.rs")));
        ui.click_on("toggle-sidebar")?;
        assert!(handle.status().get().sidebar_collapsed);
        ui.click_on("toggle-sidebar")?;
        ui.click_on("diff-file-0")?;
        assert_eq!(handle.selection().get().pane, ViewerPane::Files);
        Ok(())
    })
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
    common::run(doc, |ui, handle, _| {
        ui.advance(0.0);
        assert!(ui.stats().elements < 2_500);
        assert!(handle.handle_command(C::Last));
        let row = handle.selection().get().row.ok_or_else(|| io::Error::other("no selected last row"))?;
        let point = ui.center_of(&format!("diff-cell-{row}-New"))?;
        assert!(point.y > 0.0 && point.y < 800.0);
        assert!(ui.stats().elements < 2_500);
        assert!(handle.handle_command(C::First));
        let row = first_code(&handle)?;
        assert!(ui.center_of(&format!("diff-cell-{row}-New"))?.y < 800.0);
        Ok(())
    })
}

#[test]
fn sticky_file_tracks_scroll_and_preserves_location_across_scale_and_refresh() -> TestResult {
    let lines = "line content\n".repeat(100);
    let doc = || DocumentBuilder::new().changed("a.txt", "", &lines).changed("b.txt", "", &lines).build();
    common::run(doc(), |ui, handle, _| {
        let point = ui.center_of("diff-pane")?;
        ui.pointer_moved(point);
        ui.scroll(Point::new(0.0, -560.0));
        ui.advance(0.0);
        let header = ui.center_of("sticky-file-header")?;
        assert!(header.y > 10.0 && header.y < 120.0);
        let row = first_code(&handle)? + 29;
        let before = ui.center_of(&format!("diff-cell-{row}-New"))?;
        handle.set_document(doc());
        let after = ui.center_of(&format!("diff-cell-{row}-New"))?;
        assert!((after.y - before.y).abs() < 1.0, "{before:?} vs {after:?}");
        ui.set_size(Size::new(1400.0, 900.0));
        ui.advance(0.0);
        assert!(ui.center_of("sticky-file-header")?.y < 120.0);
        Ok(())
    })
}

#[test]
fn keyboard_reveal_clears_the_sticky_header_and_next_file_pushes_it_off() -> TestResult {
    let lines = "line content\n".repeat(100);
    let doc = DocumentBuilder::new().changed("a.txt", "", &lines).changed("b.txt", "", &lines).build();
    common::run(doc, |ui, handle, _| {
        let row = first_code(&handle)? + 20;
        let cell = format!("diff-cell-{row}-New");
        let pane = ui.bounds_of("diff-pane").ok_or_else(|| io::Error::other("no diff pane"))?;
        let before = ui.center_of(&cell)?;
        ui.pointer_moved(pane.center());
        ui.scroll(Point::new(0.0, pane.origin.y + 10.0 - before.y));
        ui.advance(0.0);
        let sticky = || io::Error::other("no sticky header");
        let header = ui.bounds_of("sticky-file-header").ok_or_else(sticky)?;
        let covered = ui.center_of(&cell)?;
        assert!(
            covered.y >= header.origin.y && covered.y < header.origin.y + header.size.height,
            "{covered:?} not in {header:?}"
        );
        assert!(handle.handle_command(C::SelectRow(row + 1)));
        ui.press(Key::ArrowUp);
        assert_eq!(handle.selection().get().row, Some(row));
        let revealed = ui.bounds_of(&cell).ok_or_else(|| io::Error::other("no revealed row"))?;
        let header = ui.bounds_of("sticky-file-header").ok_or_else(sticky)?;
        assert!(revealed.origin.y >= header.origin.y + header.size.height - 0.1);

        assert!(handle.handle_command(C::SelectFile(1)));
        ui.advance(0.0);
        let incoming = ui.bounds_of("diff-file-header-1").ok_or_else(|| io::Error::other("no next file header"))?;
        ui.scroll(Point::new(0.0, pane.origin.y + 16.0 - incoming.origin.y));
        ui.advance(0.0);
        let outgoing = ui.bounds_of("sticky-file-header").ok_or_else(sticky)?;
        let incoming = ui.bounds_of("diff-file-header-1").ok_or_else(|| io::Error::other("no next file header"))?;
        let pane = ui.bounds_of("diff-pane").ok_or_else(|| io::Error::other("no diff pane"))?;
        assert!(outgoing.origin.y < pane.origin.y && incoming.origin.y > pane.origin.y);
        assert!((outgoing.origin.y + outgoing.size.height - incoming.origin.y).abs() < 0.1);
        Ok(())
    })
}

#[test]
fn long_code_scrolls_under_fixed_gutters() -> TestResult {
    let doc = DocumentBuilder::new().changed("long.txt", "", &format!("{}\n", "long line ".repeat(250))).build();
    common::run(doc, |ui, handle, _| {
        let row = first_code(&handle)?;
        let cell = ui.center_of(&format!("diff-cell-{row}-New"))?;
        let code = format!("code-{row}-New");
        let before = ui.center_of(&code)?;
        ui.pointer_moved(cell);
        ui.scroll(Point::new(-220.0, 0.0));
        let after = ui.center_of(&code)?;
        assert!(after.x < before.x - 100.0, "{before:?} -> {after:?}");
        assert_eq!(ui.center_of(&format!("diff-cell-{row}-New"))?, cell);
        Ok(())
    })
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
    common::run(doc, |ui, handle, _| {
        assert!(ui.wait_until(|_| handle.with(ReviewController::symbols_ready)), "symbols never became ready");
        let pane = ui.bounds_of("diff-pane").ok_or_else(|| io::Error::other("no diff pane"))?;
        ui.pointer_moved(Point::new(pane.max().x - 30.0, pane.center().y));
        let (step, frames) = (60_u16, 120_u16);
        for _ in 0..10 {
            ui.scroll(Point::new(0.0, -f32::from(step)));
            ui.advance(0.016);
        }
        let start = ui.stats();
        let mut quiet = 0;
        for _ in 0..frames {
            let before = ui.stats();
            ui.scroll(Point::new(0.0, -f32::from(step)));
            ui.advance(0.016);
            let frame = ui.stats() - before;
            assert!(frame.passes <= 2, "a mount needs at most one follow-up pass: {frame:?}");
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
        let total = ui.stats() - start;
        assert!(total.elements_created <= 8 * rows, "{} elements", total.elements_created);
        assert!(total.reactive.nodes_created <= 20 * rows, "{} reactive nodes", total.reactive.nodes_created);
        Ok(())
    })
}

#[test]
fn moving_the_selection_only_repaints_the_rows_it_touches() -> TestResult {
    common::run(large_document()?, |ui, handle, _| {
        assert!(ui.wait_until(|_| handle.with(ReviewController::symbols_ready)), "symbols never became ready");
        assert!(handle.handle_command(C::SelectRow(first_code(&handle)?)));
        ui.advance(0.016);
        for _ in 0..5 {
            let before = ui.stats();
            assert!(handle.handle_command(C::MoveSelection(1)));
            ui.advance(0.016);
            let frame = ui.stats() - before;
            assert_eq!((frame.layouts, frame.elements_created, frame.elements_removed), (0, 0, 0), "{frame:?}");
            assert!(frame.layer_paints <= 4, "{frame:?}");
            assert!(frame.reactive.effects_run <= 16, "{frame:?}");
        }
        Ok(())
    })
}

#[test]
fn switching_the_theme_repaints_without_layout_or_rebuilding() -> TestResult {
    common::run(large_document()?, |ui, handle, _| {
        assert!(ui.wait_until(|_| handle.with(ReviewController::symbols_ready)), "symbols never became ready");
        let theme = ReviewTheme::builtin("github-light")?;
        let before = ui.stats();
        handle.set_theme(theme);
        ui.advance(0.016);
        let frame = ui.stats() - before;
        assert_eq!((frame.layouts, frame.elements_created, frame.elements_removed), (0, 0, 0), "{frame:?}");
        assert!(frame.layer_paints > 0, "{frame:?}");
        Ok(())
    })
}

fn large_document() -> Result<Arc<DiffDocument>, Box<dyn std::error::Error>> {
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

fn first_code(handle: &clankerdiff_rouge::ReviewHandle) -> Result<usize, io::Error> {
    handle
        .with(|s| {
            let p = s.session().presentation();
            p.rows(0..p.row_count()).iter().position(|row| row.kind == RowKind::Code)
        })
        .ok_or_else(|| io::Error::other("missing code row"))
}
