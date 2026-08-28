//! State-level contracts for the reusable GPUI viewer.

use diff_core::{DiffDocument, DiffSide, FileDiff, LineAnchor, ViewMode};
use diff_gpui::{DiffViewer, DiffViewerOptions};
use std::sync::Arc;

fn document(old: &str, new: &str) -> Arc<DiffDocument> {
    Arc::new(DiffDocument {
        repo_root: String::new(),
        files: vec![FileDiff::from_texts("src/main.rs", old, new).unwrap()],
    })
}

#[test]
fn defaults_to_auto_and_indexes_document() {
    let viewer = DiffViewer::new(document("old\n", "new\n"));
    assert_eq!(viewer.view_mode(), ViewMode::Auto);
    assert_eq!(viewer.selected_file(), Some(0));
    assert!(viewer.presentation().row_count() > 0);
    assert_eq!(viewer.review().len(), 0);
}

#[test]
fn custom_options_are_accepted() {
    let viewer = DiffViewer::with_options(
        document("old\n", "new\n"),
        diff_core::DiffTheme::default(),
        DiffViewerOptions {
            highlight_cache_capacity: 4,
            ..DiffViewerOptions::default()
        },
    );
    assert_eq!(viewer.highlight_stats().calls, 0);
}

#[test]
fn core_anchor_used_by_viewer_is_stable() {
    let document = document("old\n", "new\n");
    let anchor = LineAnchor::for_line(&document.files[0], DiffSide::New, 0, 1).unwrap();
    let same = LineAnchor::for_line(&document.files[0], DiffSide::New, 0, 1).unwrap();
    assert_eq!(anchor, same);
}
