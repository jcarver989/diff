use clankerdiff_core::{
    DiffDocument, DiffReviewCommand, DiffReviewEvent, FocusPane, Layout, RepoPath,
    RepositoryAction, StageState, ViewMode, testing::DocumentBuilder,
};
use clankerdiff_gpui::testing::{DiffViewerHarnessBuilder, MarkdownReviewerHarnessBuilder};
use clankerdiff_markdown::MarkdownReviewCommand;
use gpui::{TestAppContext, px, size};
use std::{error::Error, sync::Arc};

#[gpui::test]
fn source_hotkey_renders_one_column_and_restores_the_diff(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let harness = viewer_builder().build(cx);
        harness.update(cx, |viewer, cx| viewer.set_view_mode(ViewMode::Split, cx));
        let selected = harness.read(cx, |viewer, _| viewer.session().selected_row());
        let offset = harness.diff_scroll_top(cx);
        harness.simulate_keystrokes(cx, "o");
        let header = harness
            .bounds(cx, "source-mode-header")
            .ok_or("source header not rendered")?;
        assert!(header.size.width > px(0.0));
        assert_eq!(
            harness.read(cx, |viewer, _| viewer.layout()),
            Layout::Unified
        );
        harness.simulate_keystrokes(cx, "shift-g c");
        assert!(harness.bounds(cx, "comment-input").is_none());
        harness.simulate_keystrokes(cx, "o");
        assert!(harness.bounds(cx, "source-mode-header").is_none());
        assert_eq!(harness.read(cx, |viewer, _| viewer.layout()), Layout::Split);
        assert_eq!(
            harness.read(cx, |viewer, _| viewer.session().selected_row()),
            selected
        );
        let restored = harness.diff_scroll_top(cx);
        assert_eq!(restored.item_ix, offset.item_ix);
        assert_eq!(restored.offset_in_item, offset.offset_in_item);
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn source_round_trip_preserves_independent_scroll_across_snapshots_and_resize(
    cx: &mut TestAppContext,
) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let builder = DocumentBuilder::new().changed(
            "a.rs",
            &"let old = 1;\n".repeat(100),
            &"let new = 2;\n".repeat(100),
        );
        let document = builder.clone().build();
        for (replacement, resize) in [
            (document.clone(), false),
            (builder.clone().changed("b.rs", "old", "new").build(), false),
            (document.clone(), true),
        ] {
            let harness = DiffViewerHarnessBuilder {
                document: document.clone(),
                ..Default::default()
            }
            .build(cx);
            assert!(harness.dispatch_command(
                cx,
                DiffReviewCommand::Scroll {
                    pane: FocusPane::Diff,
                    lines: 35
                }
            )?);
            let offset = harness.diff_scroll_top(cx);
            assert!(offset.item_ix > 0);
            let selected = harness.read(cx, |viewer, _| viewer.session().selected_row());
            harness.simulate_keystrokes(cx, "o shift-g");
            let source_offset = harness.diff_scroll_top(cx);
            for equal in [document.clone(), builder.clone().build()] {
                harness.update(cx, |viewer, cx| viewer.set_document(equal, cx));
                harness.draw(cx);
                assert_eq!(harness.diff_scroll_top(cx).item_ix, source_offset.item_ix);
                assert_eq!(
                    harness.diff_scroll_top(cx).offset_in_item,
                    source_offset.offset_in_item
                );
            }
            harness.update(cx, |viewer, cx| {
                viewer.set_document(replacement, cx);
                if resize {
                    viewer.set_sidebar_width(400.0, cx);
                }
            });
            harness.draw(cx);
            harness.simulate_keystrokes(cx, "o");
            assert_eq!(harness.diff_scroll_top(cx).item_ix, offset.item_ix);
            assert_eq!(
                harness.diff_scroll_top(cx).offset_in_item,
                offset.offset_in_item
            );
            assert_eq!(
                harness.read(cx, |viewer, _| viewer.session().selected_row()),
                selected
            );
            assert!(harness.dispatch_command(cx, DiffReviewCommand::ToggleSourceView)?);
        }
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn snapshot_and_health_updates_do_not_settle_commands(cx: &mut TestAppContext) {
    let harness = viewer_builder().build(cx);
    harness.update(cx, |viewer, cx| {
        viewer.set_repository_pending(true, cx);
        viewer.set_document(document_builder().build(), cx);
        assert!(viewer.repository_pending());
        viewer.set_document(equal_document(), cx);
        assert!(viewer.repository_pending());
        viewer.set_background_error(Some("refresh failed".into()), cx);
        assert!(viewer.repository_pending());
        assert_eq!(viewer.repository_error(), Some("refresh failed"));
        viewer.set_background_error(None, cx);
        assert!(viewer.repository_pending());
        assert_eq!(viewer.repository_error(), None);
        viewer.set_repository_pending(false, cx);
        assert!(!viewer.repository_pending());
        viewer.set_repository_error("command failed", cx);
        viewer.set_background_error(Some("refresh failed".into()), cx);
        viewer.set_background_error(None, cx);
        assert_eq!(viewer.repository_error(), Some("command failed"));
    });
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

#[gpui::test]
fn replacing_an_equal_document_keeps_the_open_comment_editor(cx: &mut TestAppContext) {
    let harness = viewer_builder().build(cx);
    harness.simulate_keystrokes(cx, "c");
    assert!(harness.bounds(cx, "comment-input").is_some());

    harness.update(cx, |viewer, cx| {
        viewer.set_document(equal_document(), cx);
    });

    assert!(
        harness.bounds(cx, "comment-input").is_some(),
        "an unchanged replacement must not close the comment editor"
    );
}

#[gpui::test]
fn scrolling_the_open_theme_picker_keeps_the_background_in_place(cx: &mut TestAppContext) {
    let harness = DiffViewerHarnessBuilder {
        document: DocumentBuilder::new().generated_files(40, 60).build(),
        ..DiffViewerHarnessBuilder::default()
    }
    .build(cx);
    harness.simulate_keystrokes(cx, "t");
    assert!(
        harness.read(cx, |viewer, _| viewer.theme_picker_open()),
        "theme picker opens"
    );
    harness.scroll_to_bottom_of_diff(cx);
    let scrolled = harness.diff_scroll_top(cx);
    assert!(
        scrolled.item_ix > 0,
        "the diff list starts away from the top"
    );
    let position = harness.scroll_center(cx, "theme-picker-backdrop");
    harness.simulate_scroll(cx, position, gpui::point(gpui::px(0.0), gpui::px(240.0)));
    assert!(
        harness.read(cx, |viewer, _| viewer.theme_picker_open()),
        "scrolling the picker keeps it open"
    );
    let after = harness.diff_scroll_top(cx);
    assert_eq!(scrolled.item_ix, after.item_ix);
    assert_eq!(scrolled.offset_in_item, after.offset_in_item);
}

#[gpui::test]
fn replacing_a_document_that_drops_the_anchor_closes_the_editor(cx: &mut TestAppContext) {
    let harness = viewer_builder().build(cx);
    harness.simulate_keystrokes(cx, "c");
    assert!(harness.bounds(cx, "comment-input").is_some());

    harness.update(cx, |viewer, cx| {
        let replacement = DocumentBuilder::new()
            .changed("other.rs", "a\n", "b\n")
            .build();
        viewer.set_document(replacement, cx);
    });

    assert!(
        harness.bounds(cx, "comment-input").is_none(),
        "a dropped draft must not leave a stale comment editor open"
    );
}

#[gpui::test]
fn comment_card_dismissal_immediately_targets_the_clicked_comment(cx: &mut TestAppContext) {
    for mode in [ViewMode::Unified, ViewMode::Split] {
        DiffViewerHarnessBuilder {
            document: DocumentBuilder::new()
                .changed("src/lib.rs", "old\nother\n", "new\nsecond\n")
                .build(),
            ..Default::default()
        }
        .with_view_mode(mode)
        .with_comments(["First comment", "Second comment"])
        .run(cx, |harness, cx| {
            let original = harness.selected_row(cx);
            assert!(harness.dispatch_command(cx, DiffReviewCommand::MoveSelection(1))?);
            let selected = harness.selected_row(cx);
            assert_ne!(selected, original);
            harness.click(cx, "dismiss-comment-0")?;
            assert!(harness.review(cx).comment(0).is_none());
            assert_eq!(harness.comment_bodies(cx), ["Second comment"]);
            assert_eq!(harness.selected_row(cx), selected);
            assert!(harness.bounds(cx, "dismiss-comment-0").is_none());
            harness.simulate_keystrokes(cx, "u");
            assert!(harness.review(cx).is_empty());
            assert!(harness.bounds(cx, "dismiss-comment-1").is_none());
            Ok(())
        });
    }
}

#[gpui::test]
fn dismissing_a_queued_comment_preserves_an_active_draft(cx: &mut TestAppContext) {
    DiffViewerHarnessBuilder::default()
        .with_comments(["Queued"])
        .run(cx, |harness, cx| {
            harness.simulate_keystrokes(cx, "c");
            harness.simulate_input(cx, "Still writing");
            harness.click(cx, "dismiss-comment-0")?;
            assert!(harness.review(cx).is_empty());
            assert_eq!(harness.draft_body(cx).as_deref(), Some("Still writing"));
            assert!(harness.comment_focused(cx)?);
            harness.simulate_keystrokes(cx, "escape");
            assert!(harness.bounds(cx, "comment-input").is_none());
            Ok(())
        });
}

#[gpui::test]
fn markdown_comment_cards_use_the_same_dismissal_controls(cx: &mut TestAppContext) {
    MarkdownReviewerHarnessBuilder::default()
        .with_comments(["First", "Second"])
        .run(cx, |harness, cx| {
            let target = harness
                .read(cx, |reviewer, _| {
                    reviewer.document().targets().get(1).map(|target| target.id)
                })
                .ok_or("missing other target")?;
            assert!(harness.dispatch_command(cx, MarkdownReviewCommand::SelectTarget(target))?);
            harness.click(cx, "dismiss-comment-0")?;
            assert!(harness.review(cx).comment(0).is_none());
            assert_eq!(harness.comment_bodies(cx), ["Second"]);
            assert_eq!(harness.selected_target(cx), Some(target));
            Ok(())
        });
}

#[gpui::test]
fn dismissal_controls_support_keyboard_activation_without_viewer_actions(cx: &mut TestAppContext) {
    for key in ["enter", "space"] {
        DiffViewerHarnessBuilder::default()
            .with_comments(["Queued"])
            .run(cx, |harness, cx| {
                let selected = harness.selected_row(cx);
                for _ in 0..3 {
                    harness.focus_next(cx)?;
                }
                harness.press_key(cx, key)?;
                assert!(harness.review(cx).is_empty());
                assert_eq!(harness.selected_row(cx), selected);
                assert!(harness.events(cx).is_empty());
                Ok(())
            });
    }
}

#[gpui::test]
fn dismissal_button_fits_a_narrow_split_pane(cx: &mut TestAppContext) {
    DiffViewerHarnessBuilder::default()
        .with_window_size(700.0, 700.0)
        .with_view_mode(ViewMode::Split)
        .with_comments(["Queued"])
        .run(cx, |harness, cx| {
            harness.update(cx, |viewer, cx| viewer.set_sidebar_width(180.0, cx));
            let card = harness
                .bounds(cx, "review-comment-0")
                .ok_or("missing card")?;
            let button = harness
                .bounds(cx, "dismiss-comment-0")
                .ok_or("missing dismissal control")?;
            assert!(button.origin.x >= card.origin.x);
            assert!(button.right() <= card.right());
            assert!(button.bottom() <= card.bottom());
            harness.click(cx, "dismiss-comment-0")?;
            assert!(harness.review(cx).is_empty());
            Ok(())
        });
}

#[gpui::test]
fn dismissing_the_comment_being_edited_closes_its_editor(cx: &mut TestAppContext) {
    DiffViewerHarnessBuilder::default()
        .with_comments(["Queued"])
        .run(cx, |harness, cx| {
            harness.simulate_keystrokes(cx, "e");
            assert!(harness.bounds(cx, "comment-input").is_some());
            harness.click(cx, "dismiss-comment-0")?;
            assert!(harness.review(cx).is_empty());
            assert!(harness.draft_body(cx).is_none());
            assert!(harness.bounds(cx, "comment-input").is_none());
            Ok(())
        });
}

#[gpui::test]
fn sidebar_defaults_to_a_responsive_quarter_of_the_window(cx: &mut TestAppContext) {
    for (width, expected) in [
        (600.0, 280.0),
        (1_000.0, 320.0),
        (1_600.0, 400.0),
        (2_800.0, 700.0),
        (4_000.0, 960.0),
    ] {
        DiffViewerHarnessBuilder::default()
            .with_window_size(width, 700.0)
            .run(cx, |harness, cx| {
                let sidebar = harness
                    .bounds(cx, "diff-sidebar")
                    .ok_or("missing sidebar")?;
                assert_eq!(sidebar.size.width, px(expected));
                assert!(
                    (harness.read(cx, |viewer, _| viewer.sidebar_width()) - expected).abs()
                        < f32::EPSILON
                );
                harness.update(cx, |viewer, cx| viewer.set_sidebar_width(240.0, cx));
                harness.draw(cx);
                assert_eq!(
                    harness
                        .bounds(cx, "diff-sidebar")
                        .ok_or("missing sidebar")?
                        .size
                        .width,
                    px(240.0)
                );
                Ok(())
            });
    }
}

#[gpui::test]
fn sidebar_tracks_window_resizes_until_manually_sized(cx: &mut TestAppContext) {
    DiffViewerHarnessBuilder::default()
        .with_window_size(1_600.0, 700.0)
        .run(cx, |harness, cx| {
            cx.simulate_window_resize(harness.window(), size(px(2_000.0), px(700.0)));
            harness.draw(cx);
            assert!(
                (harness.read(cx, |viewer, _| viewer.sidebar_width()) - 500.0).abs() < f32::EPSILON
            );
            harness.update(cx, |viewer, cx| viewer.set_sidebar_width(450.0, cx));
            for (width, expected) in [(600.0, 280.0), (2_000.0, 450.0)] {
                cx.simulate_window_resize(harness.window(), size(px(width), px(700.0)));
                harness.draw(cx);
                assert!(
                    (harness.read(cx, |viewer, _| viewer.sidebar_width()) - expected).abs()
                        < f32::EPSILON
                );
            }
            Ok(())
        });
}

#[gpui::test]
fn staging_checkboxes_keep_their_geometry_and_do_not_activate_rows(cx: &mut TestAppContext) {
    for stage in [
        StageState::Unstaged,
        StageState::Staged,
        StageState::PartiallyStaged,
    ] {
        DiffViewerHarnessBuilder {
            document: DocumentBuilder::new()
                .changed_staged("src/lib.rs", "old", "new", stage)
                .changed("z.rs", "old", "new")
                .build(),
            ..Default::default()
        }
        .run(cx, |harness, cx| {
            harness.dispatch_command(cx, DiffReviewCommand::SelectFile(1))?;
            let file = harness
                .bounds(cx, "diff-file-checkbox:0")
                .ok_or("missing file checkbox")?;
            let directory = harness
                .bounds(cx, "diff-directory-checkbox:src")
                .ok_or("missing directory checkbox")?;
            assert_eq!(file.size.width, px(18.0));
            assert_eq!(file.size.height, px(18.0));
            assert_eq!(file.size, directory.size);
            harness.click(cx, "diff-file-checkbox:0")?;
            assert_eq!(
                harness.read(cx, |viewer, _| viewer.selected_file()),
                Some(1)
            );
            let expected = if stage == StageState::Staged {
                RepositoryAction::UnstagePaths(vec![RepoPath::new("src/lib.rs")?])
            } else {
                RepositoryAction::StagePaths(vec![RepoPath::new("src/lib.rs")?])
            };
            assert_eq!(
                harness.events(cx),
                vec![DiffReviewEvent::RepositoryAction(expected)]
            );
            harness.update(cx, |viewer, cx| viewer.set_repository_pending(false, cx));
            harness.click(cx, "diff-directory-checkbox:src")?;
            assert!(harness.bounds(cx, "diff-file-checkbox:0").is_some());
            assert_eq!(harness.events(cx).len(), 2);
            Ok(())
        });
    }
}

#[gpui::test]
fn staging_checkboxes_support_keyboard_activation_without_viewer_shortcuts(
    cx: &mut TestAppContext,
) {
    for key in ["enter", "space"] {
        DiffViewerHarnessBuilder {
            document: DocumentBuilder::new().changed("a.rs", "old", "new").build(),
            ..Default::default()
        }
        .run(cx, |harness, cx| {
            harness.dispatch_command(cx, DiffReviewCommand::Focus(FocusPane::Files))?;
            harness.focus_next(cx)?;
            harness.press_key(cx, key)?;
            assert_eq!(
                harness.events(cx),
                vec![DiffReviewEvent::RepositoryAction(
                    RepositoryAction::StagePaths(vec![RepoPath::new("a.rs")?])
                )]
            );
            Ok(())
        });
    }
}

fn document_builder() -> DocumentBuilder {
    DocumentBuilder::new()
        .changed_with_hunk_window(
            "src/main.rs",
            "one\ntwo\nthree\nfour\nfive\n",
            "one\ntwo\nTHREE\nfour\nfive\n",
            2..=4,
        )
        .changed("README.md", "old\n", "new\n")
}

fn equal_document() -> Arc<DiffDocument> {
    document_builder().build()
}

fn viewer_builder() -> DiffViewerHarnessBuilder {
    DiffViewerHarnessBuilder {
        document: document_builder().build(),
        ..DiffViewerHarnessBuilder::default()
    }
}
