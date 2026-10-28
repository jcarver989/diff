#![cfg(feature = "test-support")]

use clankerdiff_core::{
    CommitSummary, DiffDocument, DiffReviewEvent, DiffScope, InteractionPhase, LineStat, ReviewTarget,
    testing::{DocumentBuilder, commit_id},
};
use clankerdiff_gpui::testing::{DiffViewerHarness, DiffViewerHarnessBuilder};
use gpui::{TestAppContext, px};
use std::sync::Arc;

#[gpui::test]
fn the_header_titles_the_page_and_requests_neighbours(cx: &mut TestAppContext) {
    paged(page(commit_id(1))).run(cx, |harness, cx| {
        assert_eq!(
            harness.read(cx, |viewer, _| viewer.session().page_label().map(|label| label.to_string())),
            Some("1/3 · Commit 0000001 · Add parser".to_owned())
        );
        assert!(harness.bounds(cx, "commit-subject").is_some());
        assert!(harness.bounds(cx, "scope-staged").is_none());
        harness.click(cx, "commit-previous")?;
        harness.click(cx, "commit-next")?;
        assert_eq!(harness.events(cx), vec![DiffReviewEvent::SetTarget(commit_id(2).into())]);
        Ok(())
    });
}

#[gpui::test]
fn the_page_and_its_scopes_form_one_control_after_the_file_controls(cx: &mut TestAppContext) {
    paged(page(DiffScope::Both)).with_window_size(1400.0, 800.0).run(cx, |harness, cx| {
        let target = harness.bounds(cx, "review-target").ok_or("missing review target")?;
        let toggle = harness.bounds(cx, "diff-sidebar-toggle").ok_or("missing sidebar toggle")?;
        let collapse = harness.bounds(cx, "toggle-all-files-collapsed").ok_or("missing collapse all")?;
        let back = harness.bounds(cx, "navigate-back").ok_or("missing back")?;
        let forward = harness.bounds(cx, "navigate-forward").ok_or("missing forward")?;
        let summary = harness.bounds(cx, "diff-summary").ok_or("missing summary")?;
        let layout = harness.bounds(cx, "view-mode-toggle").ok_or("missing layout toggle")?;
        let size = harness.bounds(cx, "interface-size").ok_or("missing interface size")?;
        for part in ["commit-stepper", "scope-unstaged", "scope-staged", "scope-both"] {
            let bounds = harness.bounds(cx, part).ok_or(part)?;
            assert!(
                target.left() <= bounds.left() && bounds.right() <= target.right(),
                "{part} is outside the control"
            );
        }
        assert!(toggle.right() <= collapse.left() && collapse.left() - toggle.right() < px(4.0));
        assert!(collapse.right() < back.left() && back.left() - collapse.right() < px(16.0));
        assert!(back.right() <= forward.left() && forward.left() - back.right() < px(4.0));
        assert!(forward.right() < target.left() && target.left() - forward.right() < px(16.0));
        assert!(target.right() < layout.left() && layout.right() < size.left() && size.right() < summary.left());
        assert!(px(1400.0) - summary.right() < px(16.0));
        for bounds in [toggle, collapse, back, forward, summary, layout] {
            assert!((bounds.center().y - target.center().y).abs() < px(1.0));
        }
        assert!(harness.bounds(cx, "commit-position").is_some());
        Ok(())
    });
}

#[gpui::test]
fn a_commit_page_names_its_short_sha_and_keeps_the_summary(cx: &mut TestAppContext) {
    paged(page(commit_id(2))).with_window_size(1400.0, 800.0).run(cx, |harness, cx| {
        let short_id = harness.bounds(cx, "commit-short-id").ok_or("missing short sha")?;
        let subject = harness.bounds(cx, "commit-subject").ok_or("missing subject")?;
        assert!(short_id.right() <= subject.left());
        assert!(harness.bounds(cx, "scopes").is_none());
        assert!(harness.bounds(cx, "diff-summary").is_some());
        Ok(())
    });
}

#[gpui::test]
fn stepping_to_the_working_tree_keeps_the_arrows_in_place(cx: &mut TestAppContext) {
    paged(page(commit_id(2))).with_window_size(1400.0, 800.0).run(cx, |harness, cx| {
        let arrows = |harness: &DiffViewerHarness, cx: &mut TestAppContext| {
            ["commit-previous", "commit-next"].map(|id| harness.bounds(cx, id).map(|bounds| bounds.origin))
        };
        let before = arrows(harness, cx);
        harness.show(cx, page(DiffScope::Both));
        assert!(harness.bounds(cx, "scopes").is_some());
        assert_eq!(arrows(harness, cx), before);
        Ok(())
    });
}

#[gpui::test]
fn a_narrow_toolbar_drops_the_position_and_summary_and_keeps_its_controls_on_screen(cx: &mut TestAppContext) {
    paged(page(DiffScope::Both)).with_window_size(800.0, 700.0).run(cx, |harness, cx| {
        assert!(harness.bounds(cx, "commit-position").is_none());
        assert!(harness.bounds(cx, "diff-summary").is_none());
        let target = harness.bounds(cx, "review-target").ok_or("missing review target")?;
        let scopes = harness.bounds(cx, "scopes").ok_or("missing scopes")?;
        let layout = harness.bounds(cx, "view-mode-toggle").ok_or("missing layout toggle")?;
        let size = harness.bounds(cx, "interface-size").ok_or("missing interface size")?;
        assert!(scopes.right() <= target.right() && target.right() <= layout.left());
        assert!(size.right() <= px(800.0));
        Ok(())
    });
}

#[gpui::test]
fn the_working_tree_page_offers_its_scopes(cx: &mut TestAppContext) {
    paged(page(DiffScope::Both)).run(cx, |harness, cx| {
        harness.click(cx, "scope-staged")?;
        assert_eq!(harness.events(cx), vec![DiffReviewEvent::SetTarget(DiffScope::Staged.into())]);
        Ok(())
    });
}

#[gpui::test]
fn a_review_without_a_commit_stack_has_no_commit_stepper(cx: &mut TestAppContext) {
    DiffViewerHarnessBuilder::default().run(cx, |harness, cx| {
        assert!(harness.bounds(cx, "commit-stepper").is_none());
        assert!(harness.bounds(cx, "diff-summary").is_some());
        let target = harness.bounds(cx, "review-target").ok_or("missing review target")?;
        let scopes = harness.bounds(cx, "scopes").ok_or("missing scopes")?;
        assert!(target.left() <= scopes.left() && scopes.right() <= target.right());
        harness.simulate_keystrokes(cx, "p");
        assert!(harness.bounds(cx, "commit-picker").is_none());
        Ok(())
    });
}

#[gpui::test]
fn a_branch_without_commits_shows_the_summary_instead_of_a_stepper(cx: &mut TestAppContext) {
    let document = DocumentBuilder::new().commits_since("origin/main").changed("src/lib.rs", "old\n", "new\n").build();
    paged(document).run(cx, |harness, cx| {
        assert!(harness.bounds(cx, "commit-stepper").is_none());
        assert!(harness.bounds(cx, "diff-summary").is_some());
        Ok(())
    });
}

#[gpui::test]
fn an_empty_page_keeps_navigation_and_the_keyboard(cx: &mut TestAppContext) {
    let empty = DocumentBuilder::new().commit("Add parser").commit("Wire parser").target(commit_id(2)).build();
    paged(empty).run(cx, |harness, cx| {
        assert!(harness.bounds(cx, "changes-clean").is_some());
        assert!(harness.bounds(cx, "commit-page").is_some());
        harness.simulate_keystrokes(cx, "[");
        harness.click(cx, "commit-next")?;
        assert_eq!(
            harness.events(cx),
            vec![DiffReviewEvent::SetTarget(commit_id(1).into()), DiffReviewEvent::SetTarget(DiffScope::Both.into()),]
        );
        harness.show(cx, page(DiffScope::Both));
        assert!(harness.bounds(cx, "changes-clean").is_none());
        Ok(())
    });
}

#[gpui::test]
fn braces_jump_to_the_first_commit_and_the_working_tree(cx: &mut TestAppContext) {
    paged(page(commit_id(2))).run(cx, |harness, cx| {
        harness.simulate_keystrokes(cx, "shift-[ shift-]");
        assert_eq!(
            harness.events(cx),
            vec![DiffReviewEvent::SetTarget(commit_id(1).into()), DiffReviewEvent::SetTarget(DiffScope::Both.into())]
        );
        Ok(())
    });
}

#[gpui::test]
fn the_picker_chooses_a_page_with_the_keyboard(cx: &mut TestAppContext) {
    paged(page(commit_id(2))).run(cx, |harness, cx| {
        harness.simulate_keystrokes(cx, "p");
        assert!(harness.bounds(cx, "commit-picker").is_some());
        assert!(harness.read(cx, |viewer, _| viewer.command_context().phase == InteractionPhase::CommitPicker));
        harness.simulate_keystrokes(cx, "down enter");
        assert!(harness.bounds(cx, "commit-picker").is_none());
        let chosen = vec![DiffReviewEvent::SetTarget(DiffScope::Both.into())];
        assert_eq!(harness.events(cx), chosen);

        harness.simulate_keystrokes(cx, "p escape");
        assert!(harness.bounds(cx, "commit-picker").is_none());
        assert_eq!(harness.events(cx), chosen);
        Ok(())
    });
}

#[gpui::test]
fn the_picker_opens_from_the_header_and_chooses_with_a_click(cx: &mut TestAppContext) {
    paged(page(commit_id(2))).run(cx, |harness, cx| {
        harness.click(cx, "commit-page")?;
        assert!(harness.bounds(cx, "commit-picker-page-3").is_some());
        harness.click(cx, "commit-picker-page-1")?;
        assert!(harness.bounds(cx, "commit-picker").is_none());
        assert_eq!(harness.events(cx), vec![DiffReviewEvent::SetTarget(commit_id(1).into())]);
        Ok(())
    });
}

#[gpui::test]
fn the_picker_survives_a_refresh_but_closes_when_the_commits_change(cx: &mut TestAppContext) {
    paged(page(DiffScope::Both)).run(cx, |harness, cx| {
        harness.simulate_keystrokes(cx, "p");
        harness.show(cx, page(DiffScope::Both));
        assert!(harness.bounds(cx, "commit-picker").is_some());

        let grown = DocumentBuilder::new()
            .commit("Add parser")
            .commit("Wire parser")
            .commit("Ship parser")
            .target(DiffScope::Both)
            .changed("src/lib.rs", "old\n", "new\n")
            .build();
        harness.show(cx, grown);
        assert!(harness.bounds(cx, "commit-picker").is_none());
        assert_eq!(harness.read(cx, |viewer, _| viewer.command_context().phase), InteractionPhase::Browse);
        assert_eq!(harness.events(cx), Vec::new());
        Ok(())
    });
}

#[gpui::test]
fn the_picker_drops_down_left_aligned_under_the_control(cx: &mut TestAppContext) {
    paged(page(commit_id(2))).run(cx, |harness, cx| {
        harness.simulate_keystrokes(cx, "p");
        let target = harness.bounds(cx, "review-target").ok_or("missing review target")?;
        let picker = harness.bounds(cx, "commit-picker").ok_or("missing picker")?;
        assert!(picker.top() > target.bottom());
        assert!((picker.left() - target.left()).abs() < px(1.0));
        Ok(())
    });
}

#[gpui::test]
fn the_picker_aligns_with_the_control_on_an_empty_page(cx: &mut TestAppContext) {
    let empty = DocumentBuilder::new().commit("Add parser").commit("Wire parser").target(commit_id(2)).build();
    paged(empty).run(cx, |harness, cx| {
        harness.simulate_keystrokes(cx, "p");
        let target = harness.bounds(cx, "review-target").ok_or("missing review target")?;
        let picker = harness.bounds(cx, "commit-picker").ok_or("missing picker")?;
        assert!((picker.left() - target.left()).abs() < px(1.0));
        Ok(())
    });
}

#[gpui::test]
fn a_long_commit_picker_scrolls_to_the_working_tree_and_selects_it(cx: &mut TestAppContext) {
    let mut document = DocumentBuilder::new();
    for index in 0..41 {
        document = document.commit(&format!("Commit {index}"));
    }
    let document = document.target(commit_id(1)).changed("src/lib.rs", "old\n", "new\n").build();
    paged(document).run(cx, |harness, cx| {
        harness.simulate_keystrokes(cx, "p");
        let first = harness.bounds(cx, "commit-picker-page-1").ok_or("missing first row")?;
        let label = harness.bounds(cx, "commit-picker-page-1-label").ok_or("missing first label")?;
        assert!(harness.bounds(cx, "commit-picker-page-1-subtitle").is_none());
        assert!(first.top() <= label.top() && label.bottom() <= first.bottom());

        harness.simulate_keystrokes(cx, &"down ".repeat(41));
        let list = harness.bounds(cx, "commit-picker-list").ok_or("missing list")?;
        let working_tree = harness.bounds(cx, "commit-picker-page-42").ok_or("missing working tree row")?;
        assert!(list.top() <= working_tree.top());
        assert!(working_tree.bottom() <= list.bottom());
        assert_eq!(working_tree.size.height, first.size.height);
        let working_tree_label =
            harness.bounds(cx, "commit-picker-page-42-label").ok_or("missing working tree label")?;
        assert_eq!(working_tree_label.left(), label.left());
        harness.simulate_keystrokes(cx, "enter");
        assert!(harness.bounds(cx, "commit-picker").is_none());
        assert_eq!(harness.events(cx), vec![DiffReviewEvent::SetTarget(DiffScope::Both.into())]);
        Ok(())
    });
}

#[gpui::test]
fn the_picker_previews_the_highlighted_commit_beside_the_list(cx: &mut TestAppContext) {
    let document = DocumentBuilder::new()
        .commit_with("feat(core): add parser", |commit| CommitSummary {
            body: "Explains why the parser exists.".to_owned(),
            stat: Some(LineStat { added: 3, deleted: 1, files: 1 }),
            ..commit
        })
        .commit("Wire parser")
        .target(commit_id(1))
        .changed("src/lib.rs", "old\n", "new\n")
        .build();
    paged(document).with_window_size(1400.0, 800.0).run(cx, |harness, cx| {
        harness.simulate_keystrokes(cx, "p");
        let list = harness.bounds(cx, "commit-picker-list").ok_or("missing list")?;
        let preview = harness.bounds(cx, "commit-picker-preview").ok_or("missing preview")?;
        let eyebrow = harness.bounds(cx, "commit-preview-eyebrow").ok_or("missing eyebrow")?;
        let title = harness.bounds(cx, "commit-preview-title").ok_or("missing title")?;
        let body = harness.bounds(cx, "commit-preview-body").ok_or("missing body")?;
        let facts = harness.bounds(cx, "commit-preview-facts").ok_or("missing facts")?;
        assert!(list.right() <= preview.left());
        assert!(eyebrow.bottom() <= title.top() && title.bottom() <= body.top() && body.bottom() <= facts.top());
        assert!((preview.bottom() - facts.bottom()).abs() < px(1.0), "facts pin to the bottom of the preview");
        assert!(harness.bounds(cx, "commit-preview-change-bar").is_some());
        assert!(harness.bounds(cx, "commit-picker-page-1-marker").is_some());

        harness.simulate_keystrokes(cx, "down");
        assert!(harness.bounds(cx, "commit-preview-title").is_some());
        assert!(harness.bounds(cx, "commit-preview-body").is_none());
        assert!(harness.bounds(cx, "commit-preview-change-bar").is_none());
        let facts = harness.bounds(cx, "commit-preview-facts").ok_or("missing facts")?;
        assert!((preview.bottom() - facts.bottom()).abs() < px(1.0));
        Ok(())
    });
}

#[gpui::test]
fn comments_render_only_on_the_page_they_were_made_on(cx: &mut TestAppContext) {
    paged(page(commit_id(1))).run(cx, |harness, cx| {
        assert_eq!(harness.add_comment(cx, "on the first commit")?, 0);
        let selector = "review-comment-0";
        assert!(harness.bounds(cx, selector).is_some());
        harness.show(cx, page(commit_id(2)));
        assert!(harness.bounds(cx, selector).is_none());
        harness.show(cx, page(DiffScope::Both));
        assert!(harness.bounds(cx, selector).is_none());
        harness.show(cx, page(commit_id(1)));
        assert!(harness.bounds(cx, selector).is_some());
        assert_eq!(harness.review(cx).len(), 1);
        Ok(())
    });
}

fn page(target: impl Into<ReviewTarget>) -> Arc<DiffDocument> {
    DocumentBuilder::new()
        .commit("Add parser")
        .commit("Wire parser")
        .target(target)
        .changed("src/lib.rs", "old\n", "new\n")
        .build()
}

fn paged(document: Arc<DiffDocument>) -> DiffViewerHarnessBuilder {
    DiffViewerHarnessBuilder { document, ..DiffViewerHarnessBuilder::default() }
}
