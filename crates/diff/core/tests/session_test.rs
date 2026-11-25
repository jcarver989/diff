#![cfg(feature = "test-support")]

use clankerdiff_core::{
    CommentDraft, CommitKind, CommitPage, CommitSummary, CommitTitle, DiffDocument, DiffReviewCommand, DiffReviewEvent,
    DiffScope, DiffSide, FileDiff, FilePresentation, Layout, LineAnchor, LineStat, RevealAmount, ReviewSession,
    ReviewTarget, RowKind, SourceUnavailable, ViewMode,
    testing::{DocumentBuilder, commit_id},
};
use std::{error::Error, fmt::Write, sync::Arc, time::Duration};

#[test]
fn source_side_follows_file_status() -> Result<(), Box<dyn Error>> {
    for (builder, side) in [
        (DocumentBuilder::new().added("a.rs", "new"), DiffSide::New),
        (DocumentBuilder::new().untracked("a.rs", "new"), DiffSide::New),
        (DocumentBuilder::new().changed("a.rs", "old", "new"), DiffSide::New),
        (DocumentBuilder::new().copied("old.rs", "a.rs", "old", "new"), DiffSide::New),
        (DocumentBuilder::new().renamed("old.rs", "a.rs", "old", "new"), DiffSide::New),
        (DocumentBuilder::new().deleted("a.rs", "old"), DiffSide::Old),
        (DocumentBuilder::new().binary("a.rs"), DiffSide::New),
    ] {
        let document = builder.build();
        let mut session = ReviewSession::new(document.clone());
        assert!(session.toggle_source_view());
        assert_eq!(session.source_view().ok_or("source target")?, &document.files[0].path);
        let row = session.selected_presented_row().ok_or("source row")?;
        if document.files[0].binary {
            assert_eq!(row.kind, RowKind::FileHeader);
        } else {
            assert_eq!(row.kind, RowKind::ExpandedContext);
            assert_eq!(session.selected_side(), side);
            assert_eq!(
                row.cell(side).ok_or("source cell")?.text.as_ref(),
                if side == DiffSide::Old { "old" } else { "new" }
            );
        }
        assert!(session.toggle_source_view());
    }
    Ok(())
}

#[test]
fn equal_snapshots_reconcile_imported_comments_without_rebuilding() -> Result<(), Box<dyn Error>> {
    let builder = DocumentBuilder::new().changed("a.rs", "old", "new");
    let missing = ReviewSession::new(DocumentBuilder::new().changed("missing.rs", "gone", "missing").build())
        .selected_anchor()
        .ok_or("missing anchor")?;
    for source_view in [false, true] {
        let document = builder.clone().build();
        let mut session = ReviewSession::new(document.clone());
        if source_view {
            assert!(session.toggle_source_view());
        }
        let revision = session.projection_revision();
        let selected = session.selected_presented_row().ok_or("selected row")?.id;
        let equal = builder.clone().build();
        assert!(!Arc::ptr_eq(&document, &equal));
        for replacement in [document, equal] {
            let id = session.review_mut().add_comment(missing.clone(), "imported");
            assert!(!session.review().comment(id).ok_or("imported comment")?.outdated);
            session.set_document(replacement);
            assert!(session.review().comment(id).ok_or("reconciled comment")?.outdated);
            assert_eq!(session.projection_revision(), revision);
            assert_eq!(session.selected_presented_row().ok_or("restored row")?.id, selected);
            assert_eq!(session.source_view().is_some(), source_view);
        }
    }
    Ok(())
}

#[test]
fn source_round_trip_restores_diff_selection_layout_context_and_comments() -> Result<(), Box<dyn Error>> {
    let mut session = ReviewSession::new(
        DocumentBuilder::new()
            .changed_with_hunk_window("a.rs", "before\nold\nafter\n", "before\nnew\nafter\n", 2..=2)
            .build(),
    );
    session.set_view_mode(ViewMode::Split);
    assert!(session.toggle_full_file());
    session.set_selected_side(DiffSide::Old);
    assert!(session.begin_draft(None));
    session.draft_mut().ok_or("draft")?.set_body("saved");
    session.submit_draft().ok_or("comment")?;
    let row = session.selected_presented_row().ok_or("row")?.id;
    let side = session.selected_side();
    let rows = session.presentation().rows(0..session.presentation().row_count()).to_vec();
    assert!(session.toggle_source_view());
    assert_eq!(session.selected_side(), DiffSide::New);
    assert_eq!(session.layout(), Layout::Unified);
    assert_eq!(session.view_mode(), ViewMode::Split);
    assert_eq!(session.selected_source_line().ok_or("location")?.line_number, 2);
    assert!(!session.expand_selected(RevealAmount::All));
    assert!(!session.set_selected_side(DiffSide::Old));
    assert!(!session.move_hunk(1));
    assert!(!session.begin_draft(None));
    session.select_boundary(true);
    assert!(session.toggle_source_view());
    assert_eq!(session.layout(), Layout::Split);
    assert_eq!(session.selected_presented_row().ok_or("restored row")?.id, row);
    assert_eq!(session.selected_side(), side);
    assert_eq!(session.review().len(), 1);
    assert_eq!(session.presentation().rows(0..session.presentation().row_count()), rows);
    Ok(())
}

#[test]
fn source_survives_snapshot_reordering_and_shrink_but_not_file_removal() -> Result<(), Box<dyn Error>> {
    let mut session = ReviewSession::new(DocumentBuilder::new().changed("a.rs", "old", "one\ntwo\nthree").build());
    assert!(session.toggle_source_view());
    session.select_boundary(true);
    session
        .set_document(DocumentBuilder::new().changed("b.rs", "old", "new").changed("a.rs", "old", "one\ntwo").build());
    assert_eq!(session.selected_file(), Some(1));
    assert_eq!(session.selected_source_line().ok_or("location")?.line_number, 2);
    assert!(session.select_file(1));
    assert!(session.source_view().is_some());
    session.set_document(DocumentBuilder::new().deleted("a.rs", "deleted\nsource").build());
    assert_eq!(session.selected_side(), DiffSide::Old);
    assert_eq!(session.selected_source_line().ok_or("location")?.line_number, 2);
    assert_eq!(session.selected_presented_row().ok_or("row")?.kind, RowKind::ExpandedContext);
    session.set_document(DocumentBuilder::new().changed("b.rs", "old", "new").build());
    assert!(session.source_view().is_none());
    assert_eq!(session.selected_file(), Some(0));
    assert!(session.selected_anchor().is_some());
    Ok(())
}

#[test]
fn source_entry_uses_next_new_coordinate_and_auto_preference_survives() -> Result<(), Box<dyn Error>> {
    let mut session = ReviewSession::new(
        DocumentBuilder::new().changed("a.rs", "removed\nkeep\n", "keep\n").changed("b.rs", "old", "new").build(),
    );
    let removed = session
        .presentation()
        .rows(0..session.presentation().row_count())
        .iter()
        .position(|row| row.left.is_some())
        .ok_or("removed row")?;
    session.select_row(removed);
    assert!(session.toggle_source_view());
    assert_eq!(session.selected_source_line().ok_or("source")?.line_number, 1);
    let revision = session.projection_revision();
    assert!(!session.set_split_when_auto(true));
    assert_eq!(session.projection_revision(), revision);
    assert_eq!(session.layout(), Layout::Unified);
    assert!(session.toggle_source_view());
    assert_eq!(session.layout(), Layout::Split);
    assert!(session.toggle_source_view());
    assert!(session.select_file(1));
    assert!(session.source_view().is_none());
    assert!(session.begin_draft(None));
    assert!(!session.toggle_source_view());
    let mut empty = ReviewSession::new(DocumentBuilder::new().build());
    assert!(!empty.toggle_source_view());
    Ok(())
}

#[test]
fn edit_and_undo_preserve_comment_identity() -> Result<(), Box<dyn Error>> {
    let mut session = session();
    assert!(session.begin_draft(None));
    session.draft_mut().ok_or("missing draft")?.set_body("First comment");
    let id = session.submit_draft().ok_or("missing comment")?;
    assert!(session.edit_comment_at_selection());
    assert_eq!(session.draft().ok_or("missing edit draft")?.body(), "First comment");
    session.draft_mut().ok_or("missing draft")?.set_body("Updated comment");
    assert_eq!(session.submit_draft(), Some(id));
    assert_eq!(session.review().len(), 1);
    assert!(session.undo_last_comment());
    assert!(session.review().is_empty());
    assert!(!session.undo_last_comment());
    Ok(())
}

#[test]
fn missing_comment_operations_do_not_create_drafts() {
    let mut session = session();
    assert!(!session.edit_comment_at_selection());
    assert!(!session.delete_comment_at_selection());
    assert!(!session.undo_last_comment());
    assert!(session.draft().is_none());
    assert!(session.review().is_empty());
    let mut empty = ReviewSession::new(DocumentBuilder::new().build());
    assert!(!empty.begin_draft(None));
}

#[test]
fn blank_submission_closes_the_draft_without_adding_a_comment() {
    let mut session = session();
    assert!(session.begin_draft(None));
    assert_eq!(session.submit_draft(), None);
    assert!(session.draft().is_none());
    assert!(session.review().is_empty());
}

#[test]
fn hunk_targets_can_be_queried_without_changing_selection() -> Result<(), Box<dyn Error>> {
    let mut session = session();
    let selected = session.selected_row();
    let target = session.hunk_target(1).ok_or("missing hunk target")?;
    assert_eq!(session.selected_row(), selected);
    assert!(session.move_hunk(1));
    assert_eq!(session.selected_row(), Some(target));
    let empty = ReviewSession::new(DocumentBuilder::new().build());
    assert_eq!(empty.hunk_target(1), None);
    Ok(())
}

#[test]
fn selecting_a_row_in_another_file_updates_the_file_without_clearing_the_draft() -> Result<(), Box<dyn Error>> {
    let mut session = ReviewSession::new(
        DocumentBuilder::new().changed("a.rs", "old", "new").changed("b.rs", "before", "after").build(),
    );
    assert!(session.begin_draft(None));
    session.draft_mut().ok_or("draft")?.set_body("unfinished");
    let anchor = session.draft().ok_or("draft")?.anchor().clone();
    let row = session
        .presentation()
        .file_range(1)
        .and_then(|range| session.presentation().first_navigable(range))
        .ok_or("second file row")?;
    assert!(session.select_row(row));
    assert_eq!(session.selected_file(), Some(1));
    assert_eq!(session.selected_anchor().ok_or("anchor")?.path.as_str(), "b.rs");
    assert_eq!(session.draft().ok_or("draft")?.anchor(), &anchor);
    assert_eq!(session.draft().ok_or("draft")?.body(), "unfinished");
    session.cancel_draft();
    assert!(session.toggle_source_view());
    assert_eq!(session.presentation().file_range(0), Some(0..0));
    assert_eq!(session.selected_file(), Some(1));
    assert!(!session.select_row(usize::MAX));
    Ok(())
}

#[test]
#[allow(clippy::format_collect)]
fn files_open_on_their_first_line_rather_than_the_leading_gap() -> Result<(), Box<dyn Error>> {
    let old = (1..=20).map(|line| format!("line {line}\n")).collect::<String>();
    let mut session = ReviewSession::new(
        DocumentBuilder::new()
            .changed("a.rs", &old, &old.replace("line 15\n", "changed\n"))
            .changed("b.rs", &old, &old.replace("line 15\n", "changed\n"))
            .build(),
    );
    for file in [0, 1] {
        assert!(session.select_file(file));
        let range = session.selected_file_range().ok_or("range")?;
        let gap = range.clone().find(|&row| session.presentation().gap_info(row).is_some()).ok_or("leading gap")?;
        assert!(session.selected_row().ok_or("selection")? > gap);
        assert_eq!(session.selected_presented_row().map(|row| row.kind), Some(RowKind::Code));
    }
    Ok(())
}

#[test]
fn rows_and_files_are_laid_out_and_navigated_in_tree_order() {
    let mut session = ReviewSession::new(
        DocumentBuilder::new()
            .changed("z.rs", "old\n", "new\n")
            .binary("b.bin")
            .changed("src/a.rs", "before\n", "after\n")
            .build(),
    );
    assert_eq!(session.presentation().file_order(), [2, 1, 0]);
    assert_eq!(session.selected_file(), Some(2));
    assert!(session.select_boundary(true));
    assert_eq!(session.selected_file(), Some(0));
    assert!(!session.move_row(5));
    assert!(session.move_row(-2));
    assert_eq!(session.selected_file(), Some(2));
    assert_eq!(session.move_file(1), Some(1));
    assert_eq!(session.move_file(9), Some(0));
}

#[test]
fn collapsing_a_file_parks_selection_on_its_header_until_expanded() -> Result<(), Box<dyn Error>> {
    let mut session = two_file_session();
    assert!(session.toggle_file_collapsed(Some(0)));
    assert!(session.is_file_collapsed(0));
    let header = session.presentation().file_range(0).ok_or("range")?.start;
    assert_eq!(session.selected_row(), Some(header));
    assert_eq!(session.selected_presented_row().ok_or("row")?.kind, RowKind::FileHeader);
    assert!(session.move_row(1));
    assert_eq!(session.selected_file(), Some(1));
    assert!(session.move_row(-1));
    assert_eq!(session.selected_row(), Some(header));
    assert!(session.expand_selected(RevealAmount::Step));
    assert!(!session.is_file_collapsed(0));
    assert_eq!(session.selected_file(), Some(0));
    assert!(session.selected_anchor().is_some());
    Ok(())
}

#[test]
fn collapsing_is_refused_while_a_draft_is_open_in_the_file() -> Result<(), Box<dyn Error>> {
    let mut session = two_file_session();
    assert!(session.begin_draft(None));
    session.draft_mut().ok_or("draft")?.set_body("keep me");
    assert!(!session.toggle_file_collapsed(Some(0)));
    assert!(!session.toggle_all_files_collapsed());
    assert!(session.toggle_file_collapsed(Some(1)));
    assert_eq!(session.draft().ok_or("draft")?.body(), "keep me");
    Ok(())
}

#[test]
fn collapse_all_toggles_every_file_and_survives_refresh() -> Result<(), Box<dyn Error>> {
    let mut session = two_file_session();
    assert!(session.toggle_file_collapsed(Some(1)));
    assert!(!session.all_files_collapsed());
    assert!(session.toggle_all_files_collapsed());
    assert!(session.all_files_collapsed());
    assert_eq!(session.presentation().row_count(), 2);
    session.set_document(
        DocumentBuilder::new().changed("a.rs", "old\n", "newer\n").changed("c.rs", "old\n", "new\n").build(),
    );
    assert!(session.is_file_collapsed(0));
    assert!(!session.is_file_collapsed(1));
    assert!(session.toggle_all_files_collapsed());
    assert!(session.all_files_collapsed());
    assert!(session.toggle_all_files_collapsed());
    assert!(!session.is_file_collapsed(0));
    assert!(!session.is_file_collapsed(1));
    assert!(session.presentation().file_range(0).ok_or("range")?.len() > 1);
    Ok(())
}

#[test]
fn toggling_the_layout_flips_the_layout_auto_chose() {
    let mut session = session();
    session.set_split_when_auto(true);

    assert!(session.toggle_layout());
    assert_eq!(session.view_mode(), ViewMode::Unified);
    assert_eq!(session.layout(), Layout::Unified);

    assert!(session.toggle_layout());
    assert_eq!(session.view_mode(), ViewMode::Split);
    assert_eq!(session.layout(), Layout::Split);
}

#[test]
fn a_toggled_layout_ignores_width_until_auto_is_restored() {
    let mut session = session();
    assert!(session.toggle_layout());
    assert_eq!(session.layout(), Layout::Split);

    assert!(!session.set_split_when_auto(false));
    assert_eq!(session.layout(), Layout::Split);

    assert!(session.set_view_mode(ViewMode::Auto));
    assert_eq!(session.layout(), Layout::Unified);
}

#[test]
fn source_view_cannot_collapse_files() {
    let mut session = two_file_session();
    assert!(session.toggle_source_view());
    assert!(!session.toggle_file_collapsed(Some(0)));
    assert!(!session.toggle_all_files_collapsed());
    assert!(!session.is_file_collapsed(0));
}

#[test]
fn markdown_files_preview_in_place_of_their_diff() -> Result<(), Box<dyn Error>> {
    let readme = |new| DocumentBuilder::new().changed("README.md", "# Old\n", new).changed("a.rs", "old\n", "new\n");
    let mut session = ReviewSession::new(readme("# New\n").build());
    let preview = |session: &mut ReviewSession, index| session.toggle_markdown_preview(Some(index));
    assert!(!session.command_available(&DiffReviewCommand::ToggleMarkdownPreview(Some(1))));
    assert!(!preview(&mut session, 1));
    let source = session.selected_source_line();
    assert!(session.begin_draft(None));
    assert!(!preview(&mut session, 0));
    session.cancel_draft();
    assert!(session.toggle_file_collapsed(Some(0)));
    assert!(preview(&mut session, 0));
    let range = session.presentation().file_range(0).ok_or("range")?;
    let kinds: Vec<_> = session.presentation().rows(range.clone()).iter().map(|row| row.kind).collect();
    assert_eq!(kinds, [RowKind::FileHeader, RowKind::MarkdownPreview]);
    assert_eq!(session.selected_row(), Some(range.start));
    session.set_document(readme("# Newer\n").build());
    assert_eq!(session.presentation().file_presentation(0), FilePresentation::Markdown);
    assert!(session.expand_selected(RevealAmount::Step));
    assert_eq!(session.selected_source_line(), source);
    assert!(preview(&mut session, 0));
    session.set_document(
        readme("# New\n").source("README.md", DiffSide::New, Err::<&str, _>(SourceUnavailable::NotCaptured)).build(),
    );
    assert_eq!(session.presentation().file_presentation(0), FilePresentation::Patch);
    Ok(())
}

#[test]
fn source_preview_is_independent_of_the_diff_view_and_preserves_source_location() -> Result<(), Box<dyn Error>> {
    let doc =
        |text| DocumentBuilder::new().changed("README.md", "# Old\n", text).added("other.md", "# Other\n").build();
    let mut session = ReviewSession::new(doc("# New\n\nLast line\n"));
    let preview = DiffReviewCommand::ToggleMarkdownPreview(None);
    assert!(session.toggle_markdown_preview(None));
    assert!(session.toggle_source_view());
    assert_eq!(session.presentation().file_presentation(0), FilePresentation::Source);
    assert!(session.select_boundary(true));
    let selected = session.selected_source_line().ok_or("source line")?;
    assert!(session.command_available(&preview));
    assert!(!session.command_available(&DiffReviewCommand::ToggleMarkdownPreview(Some(1))));
    assert!(!session.toggle_markdown_preview(Some(1)));

    assert!(session.toggle_markdown_preview(None));
    session.set_document(doc("# Newer\n\nLast line\n"));
    assert_eq!(session.presentation().file_presentation(0), FilePresentation::Markdown);
    assert!(session.toggle_markdown_preview(None));
    assert_eq!(session.selected_source_line(), Some(selected.clone()));

    assert!(session.toggle_markdown_preview(None));
    assert!(session.jump_to(selected.clone()));
    assert_eq!(session.selected_source_line(), Some(selected));
    assert_eq!(session.presentation().file_presentation(0), FilePresentation::Source);
    assert!(session.toggle_source_view());
    assert_eq!(session.presentation().file_presentation(0), FilePresentation::Markdown);
    Ok(())
}

#[test]
fn source_view_disables_diff_only_commands_and_commit_picker_requires_commits() {
    let mut session = ReviewSession::new(DocumentBuilder::new().changed("a.rs", "old\n", "new\n").build());
    assert!(!session.command_available(&DiffReviewCommand::OpenCommitPicker));
    assert!(session.command_available(&DiffReviewCommand::ToggleFullFile));
    assert!(session.toggle_source_view());
    for command in [
        DiffReviewCommand::RevealGap(RevealAmount::Step),
        DiffReviewCommand::ToggleFullFile,
        DiffReviewCommand::ToggleFileCollapsed(None),
        DiffReviewCommand::ToggleAllFilesCollapsed,
        DiffReviewCommand::ToggleLayout,
        DiffReviewCommand::SetViewMode(ViewMode::Split),
        DiffReviewCommand::SelectSide(DiffSide::Old),
    ] {
        assert!(!session.command_available(&command), "{command:?}");
    }
    assert!(!session.toggle_full_file());
    assert!(!session.expand_selected(RevealAmount::Step));
    assert!(!session.undo_last_comment());

    let paged = ReviewSession::new(DocumentBuilder::new().commit("First").changed("a.rs", "old\n", "new\n").build());
    assert!(paged.command_available(&DiffReviewCommand::OpenCommitPicker));
}

fn session() -> ReviewSession {
    ReviewSession::new(DocumentBuilder::new().changed("a.rs", "old\n", "new\n").build())
}

fn two_file_session() -> ReviewSession {
    ReviewSession::new(
        DocumentBuilder::new().changed("a.rs", "old\n", "new\n").changed("b.rs", "old\n", "new\n").build(),
    )
}

#[test]
fn comments_belong_to_the_commit_they_were_made_on() -> Result<(), Box<dyn Error>> {
    let commit = page(commit_id(1), "a.rs", "new");
    let mut session = ReviewSession::new(commit.clone());
    let anchor = LineAnchor::for_line(&commit.files[0], DiffSide::New, 0, 1).ok_or("anchor")?;
    session.review_mut().add_comment_with_context(anchor.clone(), "new", "on the commit");

    session.set_document(page(DiffScope::Both, "a.rs", "new"));
    assert_eq!(session.review().len(), 1);
    assert_eq!(session.review().comments().count(), 0);
    assert!(!session.undo_last_comment());

    session.set_document(commit);
    let comment = session.review().comments_for_anchor(&anchor).next().ok_or("comment back on its commit")?;
    assert_eq!(comment.target, ReviewTarget::Commit(commit_id(1)));
    assert!(!comment.outdated);
    Ok(())
}

#[test]
fn working_tree_comments_survive_scope_changes() -> Result<(), Box<dyn Error>> {
    let both = page(DiffScope::Both, "a.rs", "new");
    let mut session = ReviewSession::new(both.clone());
    let anchor = LineAnchor::for_line(&both.files[0], DiffSide::New, 0, 1).ok_or("anchor")?;
    session.review_mut().add_comment(anchor, "keep");
    session.set_document(page(DiffScope::Staged, "a.rs", "new"));
    assert_eq!(session.review().comments().count(), 1);
    Ok(())
}

#[test]
fn navigation_steps_through_commits_and_remembers_the_scope() {
    let mut session = ReviewSession::new(page(DiffScope::Staged, "a.rs", "new"));
    assert_eq!(
        session.page_label().map(|label| label.to_string()).as_deref(),
        Some("3/3 · Uncommitted changes (staged)")
    );
    assert_eq!(session.stepped_target(1), None);
    assert_eq!(session.stepped_target(-1), Some(ReviewTarget::Commit(commit_id(2))));

    session.set_document(page(commit_id(1), "a.rs", "new"));
    assert_eq!(session.stepped_target(-1), None);
    assert_eq!(
        session.target_event(&DiffReviewCommand::StepTarget(1)),
        Some(DiffReviewEvent::SetTarget(commit_id(2).into()))
    );
    assert_eq!(
        session.target_event(&DiffReviewCommand::CycleScope),
        Some(DiffReviewEvent::SetTarget(DiffScope::Both.into()))
    );
    assert_eq!(
        session.target_event(&DiffReviewCommand::SetScope(DiffScope::Unstaged)),
        Some(DiffReviewEvent::SetTarget(DiffScope::Unstaged.into()))
    );
    assert_eq!(session.target_event(&DiffReviewCommand::Refresh), None);

    session.set_document(page(commit_id(2), "a.rs", "new"));
    assert_eq!(session.stepped_target(1), Some(ReviewTarget::WorkingTree(DiffScope::Staged)));
}

#[test]
fn pages_can_be_opened_directly() {
    let session = ReviewSession::new(page(commit_id(1), "a.rs", "new"));
    assert_eq!(session.page_target(0), None);
    assert_eq!(session.page_target(1), Some(ReviewTarget::Commit(commit_id(2))));
    assert_eq!(session.page_target(usize::MAX), Some(ReviewTarget::WorkingTree(DiffScope::Both)));
    assert_eq!(
        session.target_event(&DiffReviewCommand::GoToPage(2)),
        Some(DiffReviewEvent::SetTarget(DiffScope::Both.into()))
    );
    let unpaged = ReviewSession::new(DocumentBuilder::new().changed("a.rs", "old", "new").build());
    assert_eq!(unpaged.page_target(0), None);
    assert_eq!(unpaged.commit_pages(Duration::ZERO), None);
}

#[test]
fn commit_pages_describe_each_commit_and_the_working_tree() -> Result<(), Box<dyn Error>> {
    let now = Duration::from_hours(240);
    let document = DocumentBuilder::new()
        .commit_with("feat(core): add the first page (#12)", |commit| CommitSummary {
            body: "Why it matters.\n\nSigned-off-by: A <a@example.com>".to_owned(),
            stat: Some(LineStat { added: 12, deleted: 3, files: 2 }),
            committed_at: Some(7 * 86_400),
            ..commit
        })
        .commit("Second")
        .target(commit_id(2))
        .changed("a.rs", "old\n", "new\nmore\n")
        .build();
    let mut session = ReviewSession::new(document.clone());
    let anchor = LineAnchor::for_line(&document.files[0], DiffSide::New, 0, 1).ok_or("anchor")?;
    session.review_mut().add_comment(anchor, "second commit");

    let pages = session.commit_pages(now).ok_or("pages")?;
    let summary = |page: &CommitPage| {
        (page.number, page.id_label().to_owned(), page.stat, page.age.clone(), page.comments, page.current)
    };
    assert_eq!(
        pages.iter().map(summary).collect::<Vec<_>>(),
        [
            (
                1,
                "0000001".to_owned(),
                Some(LineStat { added: 12, deleted: 3, files: 2 }),
                Some("3d".to_owned()),
                0,
                false
            ),
            (2, "0000002".to_owned(), Some(LineStat { added: 2, deleted: 1, files: 1 }), None, 1, true),
            (3, "working tree".to_owned(), None, None, 0, false),
        ]
    );
    assert_eq!(
        pages[0].title,
        CommitTitle {
            kind: Some(CommitKind::Feature),
            scope: Some("core".to_owned()),
            summary: "Add the first page".to_owned(),
            pull_request: Some(12),
        }
    );
    assert_eq!(pages[0].body, "Why it matters.");
    assert_eq!((pages[1].title.kind, pages[1].title.summary.as_str(), pages[1].body.as_str()), (None, "Second", ""));
    assert_eq!(pages[2].subject, "Uncommitted changes");
    assert_eq!(pages[2].title.summary, "Uncommitted changes");
    assert!(pages[2].is_working_tree());
    Ok(())
}

#[test]
fn commit_submissions_group_comments_by_page() -> Result<(), Box<dyn Error>> {
    let mut session = ReviewSession::new(page(DiffScope::Both, "z.rs", "wt"));
    for (target, path, body) in [
        (DiffScope::Both.into(), "z.rs", "wt"),
        (commit_id(2).into(), "b.rs", "second"),
        (commit_id(1).into(), "a.rs", "first"),
    ] {
        let document = page::<ReviewTarget>(target, path, body);
        session.set_document(document.clone());
        let anchor = LineAnchor::for_line(&document.files[0], DiffSide::New, 0, 1).ok_or("anchor")?;
        session.review_mut().add_comment_with_context(anchor, body, body);
    }

    let formatted = session.submission().formatted;
    assert!(formatted.starts_with("I'm reviewing the commits since `origin/main` one at a time"));
    let first = formatted.find("## 1/3 · Commit 0000001 · First\n\n### `a.rs`").ok_or("first commit section")?;
    let second = formatted.find("## 2/3 · Commit 0000002 · Second\n\n### `b.rs`").ok_or("second commit section")?;
    let working_tree =
        formatted.find("## 3/3 · Uncommitted changes (both)\n\n### `z.rs`").ok_or("working tree section")?;
    assert!(first < second && second < working_tree);
    Ok(())
}

#[test]
fn working_tree_only_submissions_keep_the_plain_format() {
    let session = ReviewSession::new(page(DiffScope::Both, "a.rs", "new"));
    assert!(session.submission().formatted.starts_with("I'm reviewing the working tree diff"));
}

#[test]
fn annotations_revision_advances_only_when_comments_or_the_draft_change() -> Result<(), Box<dyn Error>> {
    let builder = DocumentBuilder::new().changed("a.rs", "old", "new");
    let mut session = ReviewSession::new(builder.clone().build());
    let initial = session.annotations_revision();
    session.cancel_draft();
    assert_eq!(session.annotations_revision(), initial);
    assert!(session.begin_draft(None));
    let drafted = session.annotations_revision();
    assert_ne!(drafted, initial);
    session.draft_mut().ok_or("draft")?.set_body("note");
    let edited = session.annotations_revision();
    assert_ne!(edited, drafted);
    session.submit_draft().ok_or("comment")?;
    let submitted = session.annotations_revision();
    assert_ne!(submitted, edited);
    session.set_document(builder.build());
    assert_eq!(session.annotations_revision(), submitted);
    session.set_document(DocumentBuilder::new().changed("b.rs", "old", "new").build());
    assert_ne!(session.annotations_revision(), submitted);
    Ok(())
}

#[test]
fn selection_starts_on_a_commentable_row() -> Result<(), Box<dyn Error>> {
    let session = multi_file_session()?;
    assert_eq!(session.selected_file(), Some(0));
    assert!(session.selected_anchor().is_some());
    Ok(())
}

#[test]
fn navigation_clamps_at_both_ends() -> Result<(), Box<dyn Error>> {
    let mut session = multi_file_session()?;
    let first = session.selected_row().ok_or("first row")?;
    session.move_row(-5);
    assert_eq!(session.selected_row(), Some(first));
    session.move_row(1_000);
    let last = session.selected_row().ok_or("last row")?;
    session.move_row(1_000);
    assert_eq!(session.selected_row(), Some(last));
    assert!(session.selected_anchor().is_some());
    Ok(())
}

#[test]
fn file_movement_resets_the_row_and_draft() -> Result<(), Box<dyn Error>> {
    let mut session = multi_file_session()?;
    assert!(session.begin_draft(None));
    assert_eq!(session.move_file(1), Some(1));
    assert!(session.draft().is_none());
    assert_eq!(session.move_file(9), Some(1));
    assert_eq!(session.move_file(-9), Some(0));
    Ok(())
}

#[test]
fn split_selection_can_target_the_removed_side() -> Result<(), Box<dyn Error>> {
    let mut session = multi_file_session()?;
    session.set_view_mode(ViewMode::Split);
    assert_eq!(session.layout(), Layout::Split);
    assert!(session.set_selected_side(DiffSide::Old));
    assert_eq!(session.selected_anchor().map(|anchor| anchor.side), Some(DiffSide::Old));
    assert!(session.set_selected_side(DiffSide::New));
    assert_eq!(session.selected_anchor().map(|anchor| anchor.side), Some(DiffSide::New));
    Ok(())
}

#[test]
fn selection_side_follows_cells_available_on_each_row() -> Result<(), Box<dyn Error>> {
    let document = Arc::new(DiffDocument {
        repo_root: "/repo".into(),
        files: vec![FileDiff::from_texts("a.rs", "old\n", "new\nextra\n")?],
        ..DiffDocument::default()
    });
    let mut session = ReviewSession::new(document);
    session.set_view_mode(ViewMode::Split);
    assert!(session.set_selected_side(DiffSide::Old));
    session.move_row(1);
    assert_eq!(session.selected_side(), DiffSide::New);
    assert!(!session.set_selected_side(DiffSide::Old));
    assert_eq!(session.selected_side(), DiffSide::New);
    Ok(())
}

#[test]
fn auto_mode_only_rebuilds_when_the_layout_changes() -> Result<(), Box<dyn Error>> {
    let mut session = multi_file_session()?;
    assert!(session.set_split_when_auto(true));
    assert_eq!(session.layout(), Layout::Split);
    assert!(!session.set_split_when_auto(true));
    assert!(session.set_view_mode(ViewMode::Split));
    assert!(!session.set_split_when_auto(false));
    assert_eq!(session.layout(), Layout::Split);
    Ok(())
}

#[test]
fn drafts_round_trip_through_the_review() -> Result<(), Box<dyn Error>> {
    let mut session = multi_file_session()?;
    assert!(session.begin_draft(None));
    session.draft_mut().ok_or("draft")?.insert("please fix");
    let id = session.submit_draft().ok_or("comment id")?;
    assert_eq!(session.review().len(), 1);
    assert_eq!(session.comment_id_at_selection(), Some(id));

    assert!(session.begin_draft(Some(id)));
    assert_eq!(session.draft().ok_or("draft")?.body(), "please fix");
    session.draft_mut().ok_or("draft")?.insert(" now");
    assert_eq!(session.submit_draft(), Some(id));
    assert_eq!(session.review().comment(id).ok_or("comment")?.body, "please fix now");
    assert_eq!(session.review().len(), 1);

    assert!(session.delete_comment_at_selection());
    assert!(session.review().is_empty());
    Ok(())
}

#[test]
fn a_blank_draft_adds_nothing() -> Result<(), Box<dyn Error>> {
    let mut session = multi_file_session()?;
    assert!(session.begin_draft(None));
    session.draft_mut().ok_or("draft")?.insert("   ");
    assert_eq!(session.submit_draft(), None);
    assert!(session.review().is_empty());
    Ok(())
}

#[test]
fn draft_editing_is_utf8_safe() -> Result<(), Box<dyn Error>> {
    let mut session = multi_file_session()?;
    assert!(session.begin_draft(None));
    let draft = session.draft_mut().ok_or("draft")?;
    draft.insert("a界b");
    draft.move_cursor_left();
    draft.delete_before_cursor();
    assert_eq!(draft.body(), "ab");
    draft.move_cursor_to_start();
    draft.delete_at_cursor();
    assert_eq!(draft.body(), "b");
    draft.move_cursor_to_end();
    assert_eq!(draft.cursor(), 1);
    Ok(())
}

#[test]
fn eager_full_file_projection_keeps_expanded_lines_non_commentable() -> Result<(), Box<dyn Error>> {
    let mut session = full_file_session();
    let selected = session.selected_source_line().ok_or("selected")?;
    assert!(session.toggle_full_file());
    assert_eq!(session.selected_source_line(), Some(selected));
    let expanded = session
        .presentation()
        .rows(0..session.presentation().row_count())
        .iter()
        .position(|row| row.kind == RowKind::ExpandedContext)
        .ok_or("expanded row")?;
    assert!(session.select_row(expanded));
    let expanded_location = session.selected_source_line().ok_or("expanded location")?;
    assert!(session.selected_anchor().is_none());
    assert!(!session.begin_draft(None));
    assert!(session.toggle_full_file());
    assert!(session.toggle_full_file());
    assert_eq!(session.selected_source_line(), Some(expanded_location));
    Ok(())
}

#[test]
fn document_replacement_preserves_an_existing_patch_selection_and_draft() -> Result<(), Box<dyn Error>> {
    let mut session = multi_file_session()?;
    session.move_row(2);
    let anchor = session.selected_anchor().ok_or("anchor")?;
    assert!(session.begin_draft(None));
    session.draft_mut().ok_or("draft")?.insert("still editing");
    let document = session.document().clone();
    session.set_document(document);
    assert_eq!(session.selected_anchor(), Some(anchor));
    assert_eq!(session.draft().map(CommentDraft::body), Some("still editing"));
    Ok(())
}

#[test]
fn document_replacement_drops_a_draft_when_content_changes_at_the_same_line() -> Result<(), Box<dyn Error>> {
    let mut session = multi_file_session()?;
    assert!(session.begin_draft(None));
    session.draft_mut().ok_or("draft")?.insert("stale draft");
    let replacement = Arc::new(DiffDocument {
        repo_root: "/repo".into(),
        files: vec![FileDiff::from_texts("a.rs", "different\ntwo\n", "CHANGED\nTWO\n")?],
        ..DiffDocument::default()
    });
    session.set_document(replacement);
    assert!(session.draft().is_none());
    Ok(())
}

#[test]
fn from_texts_documents_capture_both_sides_and_reveal_unchanged_context() -> Result<(), Box<dyn Error>> {
    let old = (1..=80).fold(String::new(), |mut text, line| {
        let _ = writeln!(text, "line {line}");
        text
    });
    let new = old.replace("line 40\n", "changed 40\n");
    let document = DiffDocument::from_texts([
        ("src/changed.rs", old.as_str(), new.as_str()),
        ("src/added.rs", "", "fn main() {}\n"),
    ])?;
    let changed = &document.files[0];
    assert_eq!(changed.hunks.len(), 1);
    assert!(changed.hunks[0].lines.len() <= 8);
    assert_eq!(changed.source_document(DiffSide::Old).ok_or("old source")?.line_count(), 80);
    assert_eq!(document.files[1].source_unavailable(DiffSide::Old), Some(&SourceUnavailable::Absent));

    let mut session = ReviewSession::new(Arc::new(document));
    reveal_from_first_code_row(&mut session)?;
    let presentation = session.presentation();
    let gap = (0..presentation.row_count()).find_map(|index| presentation.gap_info(index)).ok_or("gap")?;
    assert_eq!(gap.unavailable, None);
    Ok(())
}

#[test]
fn snapshot_replacement_retains_gap_expansions_through_an_index_shift() -> Result<(), Box<dyn Error>> {
    let (_, new) = windowed_source(&[31]);
    let fixture = windowed_fixture(&new);
    let mut session = ReviewSession::new(fixture);
    let expanded = reveal_from_first_code_row(&mut session)?;

    let (old, _) = windowed_source(&[]);
    let replacement = DocumentBuilder::new()
        .changed("inserted.rs", "one\n", "two\n")
        .changed_with_hunk_window("a.rs", &old, &new, 28..=34)
        .build();
    session.set_document(replacement);

    assert_eq!(session.selected_file(), Some(1));
    assert_eq!(expanded_row_count(&session), expanded);
    Ok(())
}

#[test]
fn snapshot_replacement_keeps_selection_in_expanded_context() -> Result<(), Box<dyn Error>> {
    let (_, new) = windowed_source(&[31]);
    let fixture = windowed_fixture(&new);
    let mut session = ReviewSession::new(fixture);
    reveal_from_first_code_row(&mut session)?;
    let expanded = session
        .presentation()
        .rows(0..session.presentation().row_count())
        .iter()
        .position(|row| row.kind == RowKind::ExpandedContext)
        .ok_or("expanded context")?;
    assert!(session.select_row(expanded));
    let source = session.selected_source_line().ok_or("source location")?;
    session.set_document(windowed_fixture(&new));
    assert_eq!(session.selected_source_line(), Some(source));
    let selected = session.selected_row().ok_or("selected row")?;
    assert_eq!(session.presentation().row(selected).ok_or("row")?.kind, RowKind::ExpandedContext);
    Ok(())
}

#[test]
fn snapshot_replacement_drops_gap_expansions_when_a_side_changes() -> Result<(), Box<dyn Error>> {
    let (_, new) = windowed_source(&[31]);
    let fixture = windowed_fixture(&new);
    let mut session = ReviewSession::new(fixture.clone());
    reveal_from_first_code_row(&mut session)?;

    let (_, edited) = windowed_source(&[31, 55]);
    let replacement = windowed_fixture(&edited);
    assert_eq!(replacement.files[0].hunks, fixture.files[0].hunks);
    session.set_document(replacement);

    assert_eq!(expanded_row_count(&session), 0);
    Ok(())
}

#[test]
fn snapshot_replacement_keeps_a_draft_when_another_file_changes() -> Result<(), Box<dyn Error>> {
    let fixture =
        DocumentBuilder::new().changed("a.rs", "one\ntwo\n", "ONE\nTWO\n").changed("b.rs", "keep\n", "kept\n").build();
    let mut session = ReviewSession::new(fixture);
    assert!(session.begin_draft(None));
    session.draft_mut().ok_or("draft")?.insert("still editing");

    let replacement = DocumentBuilder::new()
        .changed("a.rs", "one\ntwo\n", "ONE\nTWO\n")
        .changed("b.rs", "keep\n", "changed\n")
        .build();
    session.set_document(replacement);

    assert_eq!(session.draft().map(CommentDraft::body), Some("still editing"));
    Ok(())
}

#[test]
fn replacing_the_document_keeps_the_selected_path() -> Result<(), Box<dyn Error>> {
    let mut session = multi_file_session()?;
    session.move_file(1);
    let document = session.document().clone();
    session.set_document(document);
    assert_eq!(session.selected_file(), Some(1));
    Ok(())
}

fn page<T: Into<ReviewTarget>>(target: T, path: &str, new: &str) -> Arc<DiffDocument> {
    DocumentBuilder::new().commit("First").commit("Second").target(target).changed(path, "old", new).build()
}

fn multi_file_session() -> Result<ReviewSession, Box<dyn Error>> {
    Ok(ReviewSession::new(Arc::new(DiffDocument {
        repo_root: "/repo".into(),
        files: vec![
            FileDiff::from_texts("a.rs", "one\ntwo\n", "ONE\nTWO\n")?,
            FileDiff::from_texts("b.rs", "keep\n", "kept\n")?,
        ],
        ..DiffDocument::default()
    })))
}

#[allow(clippy::format_collect)]
fn full_file_session() -> ReviewSession {
    let old = (1..=60).map(|line| format!("old {line}\n")).collect::<String>();
    let new = old.replace("old 31\n", "new 31\n");
    ReviewSession::new(DocumentBuilder::new().changed_with_hunk_window("a.rs", &old, &new, 28..=34).build())
}

#[allow(clippy::format_collect)]
fn windowed_source(changed_lines: &[usize]) -> (String, String) {
    let old = (1..=60).map(|line| format!("old {line}\n")).collect::<String>();
    let new = changed_lines
        .iter()
        .fold(old.clone(), |text, line| text.replace(&format!("old {line}\n"), &format!("new {line}\n")));
    (old, new)
}

fn windowed_fixture(new_text: &str) -> Arc<DiffDocument> {
    let (old, _) = windowed_source(&[]);
    DocumentBuilder::new().changed_with_hunk_window("a.rs", &old, new_text, 28..=34).build()
}

fn expanded_row_count(session: &ReviewSession) -> usize {
    let presentation = session.presentation();
    presentation.rows(0..presentation.row_count()).iter().filter(|row| row.kind == RowKind::ExpandedContext).count()
}

fn reveal_from_first_code_row(session: &mut ReviewSession) -> Result<usize, Box<dyn Error>> {
    let code_row = session
        .presentation()
        .file_range(0)
        .ok_or("first file")?
        .find(|index| session.presentation().row(*index).is_some_and(|row| row.kind == RowKind::Code))
        .ok_or("code row")?;
    assert!(session.select_row(code_row));
    assert!(session.expand_selected(RevealAmount::Step));
    let expanded = expanded_row_count(session);
    assert!(expanded > 0);
    Ok(expanded)
}
