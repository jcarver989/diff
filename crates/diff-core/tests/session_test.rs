#![cfg(feature = "test-support")]

use clankerdiff_core::{
    DiffSide, Layout, RevealAmount, ReviewSession, RowKind, SessionOptions, ViewMode,
    testing::DocumentBuilder,
};
use std::{error::Error, sync::Arc};

#[test]
fn source_side_follows_file_status() -> Result<(), Box<dyn Error>> {
    for (builder, side) in [
        (DocumentBuilder::new().added("a.rs", "new"), DiffSide::New),
        (
            DocumentBuilder::new().untracked("a.rs", "new"),
            DiffSide::New,
        ),
        (
            DocumentBuilder::new().changed("a.rs", "old", "new"),
            DiffSide::New,
        ),
        (
            DocumentBuilder::new().copied("old.rs", "a.rs", "old", "new"),
            DiffSide::New,
        ),
        (
            DocumentBuilder::new().renamed("old.rs", "a.rs", "old", "new"),
            DiffSide::New,
        ),
        (DocumentBuilder::new().deleted("a.rs", "old"), DiffSide::Old),
        (DocumentBuilder::new().binary("a.rs"), DiffSide::New),
    ] {
        let document = builder.build();
        let mut session = ReviewSession::with_options(
            document.clone(),
            SessionOptions {
                include_file_headers: false,
            },
        );
        assert!(session.toggle_source_view());
        assert_eq!(
            session.source_view().ok_or("source target")?,
            &document.files[0].path
        );
        let row = session.selected_presented_row().ok_or("source row")?;
        if document.files[0].binary {
            assert_eq!(row.kind, RowKind::Meta);
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
    let missing = ReviewSession::new(
        DocumentBuilder::new()
            .changed("missing.rs", "gone", "missing")
            .build(),
    )
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
            let id = session
                .review_mut()
                .add_comment(missing.clone(), "imported");
            assert!(
                !session
                    .review()
                    .comment(id)
                    .ok_or("imported comment")?
                    .outdated
            );
            session.set_document(replacement);
            assert!(
                session
                    .review()
                    .comment(id)
                    .ok_or("reconciled comment")?
                    .outdated
            );
            assert_eq!(session.projection_revision(), revision);
            assert_eq!(
                session.selected_presented_row().ok_or("restored row")?.id,
                selected
            );
            assert_eq!(session.source_view().is_some(), source_view);
        }
    }
    Ok(())
}

#[test]
fn source_round_trip_restores_diff_selection_layout_context_and_comments()
-> Result<(), Box<dyn Error>> {
    let mut session = ReviewSession::new(
        DocumentBuilder::new()
            .changed_with_hunk_window(
                "a.rs",
                "before\nold\nafter\n",
                "before\nnew\nafter\n",
                2..=2,
            )
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
    let rows = session
        .presentation()
        .rows(0..session.presentation().row_count())
        .to_vec();
    assert!(session.toggle_source_view());
    assert_eq!(session.selected_side(), DiffSide::New);
    assert_eq!(session.layout(), Layout::Unified);
    assert_eq!(session.view_mode(), ViewMode::Split);
    assert_eq!(
        session
            .selected_source_line()
            .ok_or("location")?
            .line_number,
        2
    );
    assert!(!session.reveal_selected_gap(RevealAmount::All));
    assert!(!session.set_selected_side(DiffSide::Old));
    assert!(!session.move_hunk(1));
    assert!(!session.begin_draft(None));
    session.select_boundary(true);
    assert!(session.toggle_source_view());
    assert_eq!(session.layout(), Layout::Split);
    assert_eq!(
        session.selected_presented_row().ok_or("restored row")?.id,
        row
    );
    assert_eq!(session.selected_side(), side);
    assert_eq!(session.review().len(), 1);
    assert_eq!(
        session
            .presentation()
            .rows(0..session.presentation().row_count()),
        rows
    );
    Ok(())
}

#[test]
fn source_survives_snapshot_reordering_and_shrink_but_not_file_removal()
-> Result<(), Box<dyn Error>> {
    let mut session = ReviewSession::new(
        DocumentBuilder::new()
            .changed("a.rs", "old", "one\ntwo\nthree")
            .build(),
    );
    assert!(session.toggle_source_view());
    session.select_boundary(true);
    session.set_document(
        DocumentBuilder::new()
            .changed("b.rs", "old", "new")
            .changed("a.rs", "old", "one\ntwo")
            .build(),
    );
    assert_eq!(session.selected_file(), Some(1));
    assert_eq!(
        session
            .selected_source_line()
            .ok_or("location")?
            .line_number,
        2
    );
    assert!(session.select_file(1));
    assert!(session.source_view().is_some());
    session.set_document(
        DocumentBuilder::new()
            .deleted("a.rs", "deleted\nsource")
            .build(),
    );
    assert_eq!(session.selected_side(), DiffSide::Old);
    assert_eq!(
        session
            .selected_source_line()
            .ok_or("location")?
            .line_number,
        2
    );
    assert_eq!(
        session.selected_presented_row().ok_or("row")?.kind,
        RowKind::ExpandedContext
    );
    session.set_document(DocumentBuilder::new().changed("b.rs", "old", "new").build());
    assert!(session.source_view().is_none());
    assert_eq!(session.selected_file(), Some(0));
    assert!(session.selected_anchor().is_some());
    Ok(())
}

#[test]
fn source_entry_uses_next_new_coordinate_and_auto_preference_survives() -> Result<(), Box<dyn Error>>
{
    let mut session = ReviewSession::new(
        DocumentBuilder::new()
            .changed("a.rs", "removed\nkeep\n", "keep\n")
            .changed("b.rs", "old", "new")
            .build(),
    );
    let removed = session
        .presentation()
        .rows(0..session.presentation().row_count())
        .iter()
        .position(|row| row.left.is_some())
        .ok_or("removed row")?;
    session.select_row(removed);
    assert!(session.toggle_source_view());
    assert_eq!(
        session.selected_source_line().ok_or("source")?.line_number,
        1
    );
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
    session
        .draft_mut()
        .ok_or("missing draft")?
        .set_body("First comment");
    let id = session.submit_draft().ok_or("missing comment")?;
    assert!(session.edit_comment_at_selection());
    assert_eq!(
        session.draft().ok_or("missing edit draft")?.body(),
        "First comment"
    );
    session
        .draft_mut()
        .ok_or("missing draft")?
        .set_body("Updated comment");
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

fn session() -> ReviewSession {
    ReviewSession::new(
        DocumentBuilder::new()
            .changed("a.rs", "old\n", "new\n")
            .build(),
    )
}
