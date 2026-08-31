//! State-level contracts for the reusable GPUI viewer.

use diff_core::{DiffSide, Layout, LineAnchor, ViewMode, testing::DocumentBuilder};
use diff_gpui::{DiffViewer, DiffViewerOptions, default_font_size};
use diff_theme::{DiffTheme, ThemeId};

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
    assert_eq!(viewer.theme().id(), &ThemeId::Sage);
    assert!((viewer.font_size() - 16.0).abs() < f32::EPSILON);
}

#[test]
fn default_font_size_scales_with_pixel_density() {
    // Standard-density monitors keep the current default.
    assert!((default_font_size(1.0) - 16.0).abs() < f32::EPSILON);
    // Higher-density laptop screens get a proportionally smaller default.
    assert!((default_font_size(2.0) - 10.0).abs() < f32::EPSILON);
    // Intermediate densities interpolate between the two.
    assert!((default_font_size(1.5) - 10.666_667).abs() < 0.000_1);
}

#[test]
fn default_font_size_stays_within_the_supported_range() {
    // Sub-1.0 densities are treated as standard density.
    assert!((default_font_size(0.5) - 16.0).abs() < f32::EPSILON);
    // Extreme densities clamp to the viewer's minimum font size.
    assert!((default_font_size(4.0) - 10.0).abs() < f32::EPSILON);
    // Higher density never produces a larger default.
    assert!(default_font_size(1.0) >= default_font_size(1.5));
    assert!(default_font_size(1.5) >= default_font_size(2.0));
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
            sidebar_width: 360.0,
            font_size: 16.0,
            highlight_cache_capacity: 4,
            ..DiffViewerOptions::default()
        },
    );
    assert_eq!(viewer.highlight_stats().calls, 0);
    assert!((viewer.sidebar_width() - 360.0).abs() < f32::EPSILON);
    assert!((viewer.font_size() - 16.0).abs() < f32::EPSILON);
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
