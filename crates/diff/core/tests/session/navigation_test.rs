#![cfg(feature = "test-support")]

use clankerdiff_core::{
    DiffReviewCommand, DiffSide, RepoPath, ReviewSession, ReviewTarget, SourceLocation, TargetLocation,
    testing::{DocumentBuilder, commit_id, external, location, repo, source},
};
use std::{error::Error, fmt::Write};

#[test]
fn jumping_records_history_that_back_and_forward_walk() -> Result<(), Box<dyn Error>> {
    let mut session = session();
    let origin = location("src/main.rs", 2)?;
    assert!(session.jump_to(origin.clone()));
    let target = location("src/lib.rs", 2)?;
    assert!(session.jump_to(target.clone()));
    assert_eq!(session.selected_source_line(), Some(target.clone()));
    assert_eq!(session.back_location(), Some(&origin.clone().into()));
    assert!(session.navigate_back());
    assert_eq!(session.selected_source_line(), Some(origin.clone()));
    assert_eq!(session.forward_location(), Some(&target.clone().into()));
    assert!(session.navigate_forward());
    assert_eq!(session.selected_source_line(), Some(target));
    assert_eq!(session.back_location(), Some(&origin.into()));
    assert!(session.forward_location().is_none());
    assert!(!session.navigate_forward());
    Ok(())
}

#[test]
fn history_and_review_commands_are_available_only_with_something_to_act_on() -> Result<(), Box<dyn Error>> {
    let mut session = session();
    for command in [DiffReviewCommand::NavigateBack, DiffReviewCommand::NavigateForward, DiffReviewCommand::CopyReview]
    {
        assert!(!session.command_available(&command), "{command:?}");
    }
    assert!(session.command_available(&DiffReviewCommand::Refresh));
    assert!(session.jump_to(location("src/lib.rs", 2)?));
    assert!(session.command_available(&DiffReviewCommand::NavigateBack));
    assert!(session.navigate_back());
    assert!(session.command_available(&DiffReviewCommand::NavigateForward));
    let anchor = session.selected_anchor().ok_or("a commentable line")?;
    session.review_mut().add_comment(anchor, "note");
    assert!(session.command_available(&DiffReviewCommand::CopyReview));
    assert!(!session.command_available(&DiffReviewCommand::StepTarget(1)), "a working tree has no commits to step");
    Ok(())
}

#[test]
fn jumping_outside_the_diff_waits_for_the_source_then_shows_it_read_only() -> Result<(), Box<dyn Error>> {
    let mut session = session();
    let origin = location("src/main.rs", 2)?;
    assert!(session.jump_to(origin.clone()));
    let outside = TargetLocation { path: external(4, "std · alloc/src/sync.rs"), side: DiffSide::New, line_number: 2 };
    assert!(session.jump_to(outside.clone()));
    assert_eq!(session.pending_reference(), Some(&outside));
    assert_eq!(session.reference_source(), Some(&outside.path));
    assert_eq!(session.current_location(), Some(outside.clone()));
    assert_eq!(session.reference_status().as_deref(), Some("Opening std · alloc/src/sync.rs…"));
    session.reference_failed(&repo("src/other.rs")?, "elsewhere");
    session.reference_failed(&outside.path, "not readable");
    assert_eq!(session.reference_status().as_deref(), Some("Couldn't open std · alloc/src/sync.rs: not readable"));
    assert!(!session.open_reference(&repo("src/other.rs")?, source(UTIL)));
    assert!(session.open_reference(&outside.path, source(UTIL)));
    assert!(session.pending_reference().is_none());
    assert_eq!(session.reference_source(), Some(&outside.path));
    assert_eq!(session.reference().map(|file| file.path.as_str()), Some("std · alloc/src/sync.rs"));
    assert!(session.reference_status().is_none());
    assert_eq!(session.current_location(), Some(outside.clone()));
    assert!(session.shows_external(&outside.path.display_path()));
    assert!(session.move_row(1));
    assert!(session.selected_anchor().is_none());
    assert!(session.navigate_back());
    assert!(session.reference().is_none());
    assert_eq!(session.selected_source_line(), Some(origin));
    assert!(session.navigate_forward());
    assert_eq!(session.pending_reference(), Some(&TargetLocation { line_number: 3, ..outside }));
    Ok(())
}

#[test]
fn navigating_back_from_a_reference_shows_the_whole_diff_again() -> Result<(), Box<dyn Error>> {
    let mut session = session();
    let diff_rows = session.presentation().row_count();
    assert!(session.jump_to(location("src/main.rs", 2)?));
    let outside = TargetLocation { path: external(4, "std · alloc/src/sync.rs"), side: DiffSide::New, line_number: 2 };
    assert!(session.jump_to(outside.clone()));
    assert!(session.open_reference(&outside.path, source(UTIL)));
    let reference_revision = session.projection_revision();
    assert!(session.navigate_back());
    assert_ne!(session.projection_revision(), reference_revision);
    assert_eq!(session.presentation().row_count(), diff_rows);
    assert_eq!(session.visible_range(), 0..diff_rows);
    Ok(())
}

#[test]
fn leaving_a_reference_for_another_file_returns_to_the_diff() -> Result<(), Box<dyn Error>> {
    let mut session = session();
    let origin = location("src/main.rs", 2)?;
    assert!(session.jump_to(origin.clone()));
    assert!(session.jump_to(location("src/util.rs", 2)?));
    assert!(session.open_reference(&repo("src/util.rs")?, source(UTIL)));
    assert_eq!(session.selected_source_line(), Some(location("src/util.rs", 2)?));
    assert!(session.select_file(0));
    assert!(session.reference().is_none());
    assert!(session.source_view().is_none());
    assert!(session.jump_to(location("src/util.rs", 2)?));
    let row = session.presentation().row_showing_source(&origin).ok_or("row")?;
    assert!(session.select_row(row));
    assert!(session.pending_reference().is_none());
    Ok(())
}

#[test]
fn only_new_side_locations_outside_the_diff_can_be_opened() -> Result<(), Box<dyn Error>> {
    let mut session = session();
    let old = SourceLocation { side: DiffSide::Old, ..location("src/util.rs", 3)? };
    assert!(!session.jump_to(old));
    assert!(!session.jump_to(location("src/util.rs", 0)?));
    assert!(session.pending_reference().is_none());
    Ok(())
}

#[test]
fn jumping_into_hidden_context_reveals_only_the_lines_around_the_target() -> Result<(), Box<dyn Error>> {
    let filler = |range: std::ops::Range<usize>| {
        range.fold(String::new(), |mut lines, index| {
            let _ = writeln!(lines, "// line {index}");
            lines
        })
    };
    let body = format!("{}fn target() {{}}\n{}", filler(2..20), filler(21..61));
    let old = format!("fn head() {{}}\n{body}fn tail() {{}}\n");
    let new = format!("fn head() {{ target(); }}\n{body}fn tail() {{ target(); }}\n");
    let mut session = ReviewSession::new(DocumentBuilder::new().changed("src/lib.rs", &old, &new).build());
    let target = location("src/lib.rs", 20)?;
    assert!(session.presentation().row_showing_source(&target).is_none());
    assert!(session.jump_to(target.clone()));
    assert_eq!(session.selected_source_line(), Some(target));
    assert!(session.presentation().row_showing_source(&location("src/lib.rs", 23)?).is_some());
    assert!(session.presentation().row_showing_source(&location("src/lib.rs", 40)?).is_none());
    Ok(())
}

#[test]
fn jumping_into_a_collapsed_file_expands_it() -> Result<(), Box<dyn Error>> {
    let mut session = session();
    assert!(session.toggle_file_collapsed(1));
    let target = location("src/main.rs", 1)?;
    assert!(session.jump_to(target.clone()));
    assert!(!session.is_file_collapsed(1));
    assert_eq!(session.selected_source_line(), Some(target));
    Ok(())
}

#[test]
fn a_reference_survives_snapshots_stays_open_once_its_file_joins_the_diff_and_closes_when_the_target_changes()
-> Result<(), Box<dyn Error>> {
    let mut session = session();
    assert!(session.jump_to(location("src/main.rs", 2)?));
    assert!(session.jump_to(location("src/util.rs", 3)?));
    assert!(session.open_reference(&repo("src/util.rs")?, source(UTIL)));
    session.set_document(
        DocumentBuilder::new().changed("src/lib.rs", LIB, &format!("{LIB}\n")).changed("src/main.rs", "", MAIN).build(),
    );
    assert!(session.reference().is_some());
    assert_eq!(session.selected_source_line(), Some(location("src/util.rs", 3)?));
    assert!(session.back_location().is_some());
    session
        .set_document(DocumentBuilder::new().changed("src/main.rs", "", MAIN).changed("src/util.rs", "", UTIL).build());
    assert!(session.reference().is_none());
    assert_eq!(session.source_view().map(RepoPath::as_str), Some("src/util.rs"));
    assert_eq!(session.selected_source_line(), Some(location("src/util.rs", 3)?));
    assert!(session.back_location().is_some());
    assert!(session.jump_to(location("src/lib.rs", 1)?));
    session.set_document(
        DocumentBuilder::new().target(ReviewTarget::Commit(commit_id(1))).changed("src/main.rs", "", MAIN).build(),
    );
    assert!(session.reference().is_none());
    assert!(session.back_location().is_none());
    Ok(())
}

const LIB: &str = "pub fn build() -> u32 {\n    helper()\n}\n";
const MAIN: &str = "fn main() {\n    build();\n    helper();\n}\n";
const UTIL: &str = "fn util() {\n    one();\n    two();\n}\n";

fn session() -> ReviewSession {
    ReviewSession::new(
        DocumentBuilder::new()
            .changed("src/lib.rs", "", "\npub fn build() -> u32 {\n    helper()\n}\nfn helper() -> u32 { 1 }\n")
            .changed("src/main.rs", "", MAIN)
            .build(),
    )
}
