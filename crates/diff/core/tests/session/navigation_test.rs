#![cfg(feature = "test-support")]

use clankerdiff_core::{
    ReviewSession, ReviewTarget,
    testing::{DocumentBuilder, commit_id, location},
};
use std::{error::Error, fmt::Write};

#[test]
fn jumping_records_history_that_back_and_forward_walk() -> Result<(), Box<dyn Error>> {
    let mut session = session();
    let origin = location("src/main.rs", 2)?;
    assert!(session.jump_to(&origin));
    let target = location("src/lib.rs", 2)?;
    assert!(session.jump_to(&target));
    assert_eq!(session.selected_source_line(), Some(target.clone()));
    assert_eq!(session.back_location(), Some(&origin));
    assert!(session.navigate_back());
    assert_eq!(session.selected_source_line(), Some(origin.clone()));
    assert_eq!(session.forward_location(), Some(&target));
    assert!(session.navigate_forward());
    assert_eq!(session.selected_source_line(), Some(target));
    assert_eq!(session.back_location(), Some(&origin));
    assert!(session.forward_location().is_none());
    assert!(!session.navigate_forward());
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
    assert!(session.jump_to(&target));
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
    assert!(session.jump_to(&target));
    assert!(!session.is_file_collapsed(1));
    assert_eq!(session.selected_source_line(), Some(target));
    Ok(())
}

#[test]
fn history_survives_snapshots_until_the_review_target_changes() -> Result<(), Box<dyn Error>> {
    let mut session = session();
    assert!(session.jump_to(&location("src/main.rs", 2)?));
    assert!(session.jump_to(&location("src/lib.rs", 2)?));
    session.set_document(
        DocumentBuilder::new().changed("src/lib.rs", LIB, &format!("{LIB}\n")).changed("src/main.rs", "", MAIN).build(),
    );
    assert_eq!(session.back_location(), Some(&location("src/main.rs", 2)?));
    session.set_document(
        DocumentBuilder::new().target(ReviewTarget::Commit(commit_id(1))).changed("src/main.rs", "", MAIN).build(),
    );
    assert!(session.back_location().is_none());
    Ok(())
}

#[test]
fn locations_outside_the_diff_cannot_be_jumped_to() -> Result<(), Box<dyn Error>> {
    let mut session = session();
    assert!(!session.jump_to(&location("src/util.rs", 2)?));
    assert!(session.back_location().is_none());
    Ok(())
}

const LIB: &str = "pub fn build() -> u32 {\n    helper()\n}\n";
const MAIN: &str = "fn main() {\n    build();\n    helper();\n}\n";

fn session() -> ReviewSession {
    ReviewSession::new(
        DocumentBuilder::new()
            .changed("src/lib.rs", "", "\npub fn build() -> u32 {\n    helper()\n}\nfn helper() -> u32 { 1 }\n")
            .changed("src/main.rs", "", MAIN)
            .build(),
    )
}
