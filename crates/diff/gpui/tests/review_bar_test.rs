#![cfg(feature = "test-support")]

use clankerdiff_core::{
    DiffDocument, DiffReviewEvent, DiffScope, ReviewTarget,
    testing::{DocumentBuilder, commit_id},
};
use clankerdiff_gpui::testing::DiffViewerHarnessBuilder;
use gpui::TestAppContext;
use std::sync::Arc;

#[gpui::test]
fn the_stepper_titles_the_page_and_requests_neighbours(cx: &mut TestAppContext) {
    paged(page(commit_id(1))).run(cx, |harness, cx| {
        assert_eq!(
            harness.read(cx, |viewer, _| viewer.session().page_title()),
            Some("1/3 · Commit 0000001 · Add parser".to_owned())
        );
        harness.click(cx, "commit-previous")?;
        harness.click(cx, "commit-next")?;
        harness.click(cx, "scope-staged")?;
        assert_eq!(
            harness.events(cx),
            vec![
                DiffReviewEvent::SetTarget(commit_id(2).into()),
                DiffReviewEvent::SetTarget(DiffScope::Staged.into()),
            ]
        );
        Ok(())
    });
}

#[gpui::test]
fn a_review_without_a_commit_stack_has_no_stepper(cx: &mut TestAppContext) {
    DiffViewerHarnessBuilder::default().run(cx, |harness, cx| {
        assert!(harness.bounds(cx, "commit-stepper").is_none());
        assert!(harness.bounds(cx, "scope-staged").is_some());
        Ok(())
    });
}

#[gpui::test]
fn an_empty_page_keeps_navigation_and_the_keyboard(cx: &mut TestAppContext) {
    let empty = DocumentBuilder::new()
        .commit("Add parser")
        .commit("Wire parser")
        .target(commit_id(2))
        .build();
    paged(empty).run(cx, |harness, cx| {
        assert!(harness.bounds(cx, "changes-clean").is_some());
        assert!(harness.bounds(cx, "commit-page").is_some());
        harness.simulate_keystrokes(cx, "[");
        harness.click(cx, "commit-next")?;
        assert_eq!(
            harness.events(cx),
            vec![
                DiffReviewEvent::SetTarget(commit_id(1).into()),
                DiffReviewEvent::SetTarget(DiffScope::Both.into()),
            ]
        );
        harness.show(cx, page(DiffScope::Both));
        assert!(harness.bounds(cx, "changes-clean").is_none());
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
    DiffViewerHarnessBuilder {
        document,
        ..DiffViewerHarnessBuilder::default()
    }
}
