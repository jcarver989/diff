#![cfg(feature = "test-support")]

use clankerdiff_core::{
    DiffScope, ReviewTarget,
    testing::{DocumentBuilder, commit_id},
};
use std::error::Error;

#[test]
fn pages_are_each_commit_then_the_working_tree() -> Result<(), Box<dyn Error>> {
    let document = DocumentBuilder::new().commit("First").commit("Second").build();
    let stack = document.commits.as_deref().ok_or("commit stack")?;
    assert_eq!(stack.page_count(), 3);
    assert_eq!(stack.page(0, DiffScope::Both), Some(ReviewTarget::Commit(commit_id(1))));
    assert_eq!(stack.page(2, DiffScope::Staged), Some(ReviewTarget::WorkingTree(DiffScope::Staged)));
    assert_eq!(stack.page(3, DiffScope::Both), None);
    assert_eq!(stack.page_index(&ReviewTarget::WorkingTree(DiffScope::Unstaged)), Some(2));
    assert_eq!(stack.page_index(&ReviewTarget::Commit(commit_id(9))), None);
    Ok(())
}
