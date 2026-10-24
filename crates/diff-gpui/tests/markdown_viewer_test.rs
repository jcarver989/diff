use clankerdiff_core::ReviewCommand;
use clankerdiff_gpui::{
    MarkdownReviewerOptions,
    testing::{MarkdownReviewerHarness, MarkdownReviewerHarnessBuilder},
};
use clankerdiff_markdown::{MarkdownReviewDecision, MarkdownReviewEvent, MarkdownTargetId};
use gpui::{TestAppContext, px};

const PLAN: &str = "# Plan\n\nIntro with **bold** text.\n\n## Options\n\n- Postgres\n- DynamoDB\n\n```rust\nlet a = 1;\nlet b = 2;\nlet c = 3;\n```\n\n## Decision\n\nPick one.";

#[gpui::test]
fn code_block_lines_render_as_one_contiguous_card(cx: &mut TestAppContext) {
    plan().run(cx, |harness, cx| {
        let first = harness
            .bounds(cx, "markdown-row-6")
            .ok_or("missing first code line")?;
        let second = harness
            .bounds(cx, "markdown-row-7")
            .ok_or("missing second code line")?;
        let last = harness
            .bounds(cx, "markdown-row-8")
            .ok_or("missing last code line")?;
        assert_eq!(first.bottom(), second.origin.y);
        assert_eq!(second.bottom(), last.origin.y);
        assert_eq!(first.size.height, second.size.height);
        assert!(first.size.height <= px(24.0));
        Ok(())
    });
}

#[gpui::test]
fn headings_render_larger_than_body_text(cx: &mut TestAppContext) {
    plan().run(cx, |harness, cx| {
        let heading = harness
            .bounds(cx, "markdown-row-0")
            .ok_or("missing heading")?;
        let paragraph = harness
            .bounds(cx, "markdown-row-1")
            .ok_or("missing paragraph")?;
        assert!(heading.size.height > paragraph.size.height);
        Ok(())
    });
}

#[gpui::test]
fn block_quotes_render_their_nested_blocks_in_one_row(cx: &mut TestAppContext) {
    MarkdownReviewerHarnessBuilder::default()
        .with_source("# Quote\n\n> First line.\n>\n> - nested item\n\nAfter.")
        .run(cx, |harness, cx| {
            let quote = harness
                .bounds(cx, "markdown-row-1")
                .ok_or("missing quote")?;
            let after = harness
                .bounds(cx, "markdown-row-2")
                .ok_or("missing paragraph")?;
            assert!(quote.size.height > after.size.height * 1.5);
            assert!(quote.bottom() <= after.origin.y);
            Ok(())
        });
}

#[gpui::test]
fn document_text_is_centered_in_a_reading_column(cx: &mut TestAppContext) {
    plan()
        .with_window_size(1800.0, 900.0)
        .run(cx, |harness, cx| {
            let document = harness
                .bounds(cx, "markdown-document")
                .ok_or("missing document")?;
            let paragraph = harness
                .bounds(cx, "markdown-row-1")
                .ok_or("missing paragraph")?;
            assert!(paragraph.size.width < px(900.0));
            let left = paragraph.origin.x - document.origin.x;
            let right = document.right() - paragraph.right();
            assert!((left - right).abs() <= px(16.0));
            Ok(())
        });
}

#[gpui::test]
fn header_shows_the_artifact_title_and_path(cx: &mut TestAppContext) {
    plan()
        .with_metadata(Some("docs/plan.md"), Some("Choose a database"))
        .run(cx, |harness, cx| {
            assert!(harness.bounds(cx, "markdown-title").is_some());
            assert!(harness.bounds(cx, "markdown-path").is_some());
            Ok(())
        });
    plan().run(cx, |harness, cx| {
        assert!(harness.bounds(cx, "markdown-title").is_some());
        assert!(harness.bounds(cx, "markdown-path").is_none());
        Ok(())
    });
}

#[gpui::test]
fn primary_action_approves_a_review_without_comments(cx: &mut TestAppContext) {
    plan().run(cx, |harness, cx| {
        harness.click(cx, "markdown-submit")?;
        assert_eq!(
            submitted_decision(&harness.events(cx)),
            Some(MarkdownReviewDecision::Approved)
        );
        Ok(())
    });
}

#[gpui::test]
fn primary_action_sends_feedback_when_there_are_comments(cx: &mut TestAppContext) {
    plan()
        .with_comments(["Prefer Postgres"])
        .run(cx, |harness, cx| {
            harness.click(cx, "markdown-submit")?;
            assert_eq!(
                submitted_decision(&harness.events(cx)),
                Some(MarkdownReviewDecision::ChangesRequested)
            );
            Ok(())
        });
}

#[gpui::test]
fn hovering_a_block_offers_an_add_comment_button(cx: &mut TestAppContext) {
    plan().run(cx, |harness, cx| {
        harness.hover(cx, "markdown-row-1")?;
        harness.click(cx, "markdown-add-comment-1")?;
        assert_eq!(harness.selected_target(cx), Some(target(harness, cx, 1)?));
        assert!(harness.bounds(cx, "comment-input").is_some());
        Ok(())
    });
}

#[gpui::test]
fn saved_comments_render_in_a_thread_below_their_block(cx: &mut TestAppContext) {
    plan().with_comments(["Looks good"]).run(cx, |harness, cx| {
        let heading = harness
            .bounds(cx, "markdown-row-0")
            .ok_or("missing heading")?;
        let thread = harness
            .bounds(cx, "comment-thread-0")
            .ok_or("missing thread")?;
        assert!(thread.origin.y >= heading.origin.y);
        assert!(thread.bottom() <= heading.bottom());
        Ok(())
    });
}

#[gpui::test]
fn theme_controls_follow_the_themes_option(cx: &mut TestAppContext) {
    plan().run(cx, |harness, cx| {
        assert!(harness.bounds(cx, "markdown-theme").is_some());
        Ok(())
    });
    plan()
        .with_options(MarkdownReviewerOptions {
            themes: false,
            ..MarkdownReviewerOptions::default()
        })
        .run(cx, |harness, cx| {
            assert!(harness.bounds(cx, "markdown-theme").is_none());
            harness.simulate_keystrokes(cx, "t");
            assert!(!harness.read(cx, |reviewer, _| {
                reviewer.command_enabled(&ReviewCommand::OpenThemePicker.into())
            }));
            Ok(())
        });
}

#[gpui::test]
fn question_mark_toggles_the_shortcut_help(cx: &mut TestAppContext) {
    plan().run(cx, |harness, cx| {
        harness.simulate_keystrokes(cx, "shift-/");
        assert!(harness.bounds(cx, "shortcut-help-backdrop").is_some());
        harness.simulate_keystrokes(cx, "escape");
        assert!(harness.bounds(cx, "shortcut-help-backdrop").is_none());
        assert_eq!(harness.events(cx), []);
        harness.click(cx, "markdown-shortcuts")?;
        assert!(harness.bounds(cx, "shortcut-help-backdrop").is_some());
        Ok(())
    });
}

#[gpui::test]
fn outline_is_collapsible(cx: &mut TestAppContext) {
    plan().run(cx, |harness, cx| {
        assert!(harness.bounds(cx, "markdown-outline").is_some());
        harness.click(cx, "markdown-outline-toggle")?;
        assert!(harness.bounds(cx, "markdown-outline").is_none());
        harness.click(cx, "markdown-outline-toggle")?;
        assert!(harness.bounds(cx, "markdown-outline").is_some());
        Ok(())
    });
}

#[gpui::test]
fn outline_is_hidden_for_documents_with_one_heading(cx: &mut TestAppContext) {
    MarkdownReviewerHarnessBuilder::default().run(cx, |harness, cx| {
        assert!(harness.bounds(cx, "markdown-outline").is_none());
        assert!(harness.bounds(cx, "markdown-outline-toggle").is_none());
        Ok(())
    });
}

#[gpui::test]
fn empty_documents_show_an_empty_state(cx: &mut TestAppContext) {
    MarkdownReviewerHarnessBuilder::default()
        .with_source("")
        .run(cx, |harness, cx| {
            assert!(harness.bounds(cx, "markdown-empty").is_some());
            Ok(())
        });
}

fn plan() -> MarkdownReviewerHarnessBuilder {
    MarkdownReviewerHarnessBuilder::default().with_source(PLAN)
}

fn target(
    harness: &MarkdownReviewerHarness,
    cx: &TestAppContext,
    index: usize,
) -> Result<MarkdownTargetId, &'static str> {
    harness
        .read(cx, |reviewer, _| {
            reviewer
                .document()
                .targets()
                .get(index)
                .map(|target| target.id)
        })
        .ok_or("missing target")
}

fn submitted_decision(events: &[MarkdownReviewEvent]) -> Option<MarkdownReviewDecision> {
    events.iter().rev().find_map(|event| match event {
        MarkdownReviewEvent::Submit(submission) => Some(submission.decision),
        _ => None,
    })
}
