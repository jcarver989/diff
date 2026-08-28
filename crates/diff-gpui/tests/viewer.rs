//! State-level contracts for the reusable GPUI viewer.

use diff_core::{DiffSide, DiffTheme, Layout, LineAnchor, ViewMode, testing::DocumentBuilder};
use diff_gpui::{DiffViewer, DiffViewerOptions};

#[test]
fn defaults_to_auto_and_indexes_document() {
    let viewer = DiffViewer::new(
        DocumentBuilder::new()
            .changed("src/main.rs", "old\n", "new\n")
            .build(),
    );
    assert_eq!(viewer.view_mode(), ViewMode::Auto);
    assert_eq!(viewer.layout(), Layout::Unified);
    assert_eq!(viewer.selected_file(), Some(0));
    assert!(viewer.presentation().row_count() > 0);
    assert_eq!(viewer.review().len(), 0);
}

#[test]
fn the_sticky_header_replaces_the_file_header_row() {
    let viewer = DiffViewer::new(
        DocumentBuilder::new()
            .changed("src/main.rs", "old\n", "new\n")
            .build(),
    );
    assert!(
        viewer
            .presentation()
            .rows(0..viewer.presentation().row_count())
            .iter()
            .all(|row| row.kind != diff_core::RowKind::FileHeader),
        "the diff pane draws its own file header"
    );
}

#[test]
fn custom_options_are_accepted() {
    let viewer = DiffViewer::with_options(
        DocumentBuilder::new()
            .changed("src/main.rs", "old\n", "new\n")
            .build(),
        DiffTheme::default(),
        DiffViewerOptions {
            highlight_cache_capacity: 4,
            ..DiffViewerOptions::default()
        },
    );
    assert_eq!(viewer.highlight_stats().calls, 0);
}

#[test]
fn the_session_drives_selection_and_review() {
    let mut viewer = DiffViewer::new(
        DocumentBuilder::new()
            .changed("src/main.rs", "old\n", "new\n")
            .changed("README.md", "a\n", "b\n")
            .build(),
    );
    let session = viewer.session_mut();
    assert!(session.move_file(1).is_some());
    assert_eq!(session.selected_file(), Some(1));
    assert!(session.begin_draft(None));
    session.draft_mut().unwrap().insert("looks wrong");
    assert!(session.submit_draft().is_some());
    assert_eq!(viewer.review().len(), 1);
    assert!(
        viewer
            .session()
            .submission()
            .formatted
            .contains("looks wrong")
    );
}

#[test]
fn core_anchor_used_by_viewer_is_stable() {
    let document = DocumentBuilder::new()
        .changed("src/main.rs", "old\n", "new\n")
        .build();
    let anchor = LineAnchor::for_line(&document.files[0], DiffSide::New, 0, 1).unwrap();
    let same = LineAnchor::for_line(&document.files[0], DiffSide::New, 0, 1).unwrap();
    assert_eq!(anchor, same);
}
