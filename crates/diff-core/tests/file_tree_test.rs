#![cfg(feature = "test-support")]

use clankerdiff_core::{
    DiffDocument, FileTree, RepoPath, StageState, testing::DocumentBuilder, tree_order,
};
use std::sync::Arc;

#[test]
fn builds_a_sorted_tree_while_preserving_document_indices() {
    let document = nested_document();
    let tree = FileTree::new(&document);
    assert_eq!(
        labels(&tree),
        [
            "0:crates/",
            "1:crates/z/",
            "2:crates/z/src/",
            "3:crates/z/src/lib.rs#1",
            "1:crates/a.rs#2",
            "0:src/",
            "1:src/lib.rs#4",
            "1:src/main.rs#3",
            "0:README.md#0",
        ]
    );
    assert_eq!(tree_order(&document), [1, 2, 4, 3, 0]);
}

#[test]
fn collapsing_hides_descendants_and_rebuild_prunes_removed_directories() {
    let mut tree = FileTree::new(&nested_document());
    tree.toggle("crates");
    tree.set_expanded("crates/z", false);
    assert_eq!(
        labels(&tree),
        [
            "0:crates/-",
            "0:src/",
            "1:src/lib.rs#4",
            "1:src/main.rs#3",
            "0:README.md#0"
        ]
    );
    tree.toggle("crates");
    assert!(labels(&tree).contains(&"1:crates/z/-".to_owned()));
    tree.rebuild(
        &DocumentBuilder::new()
            .changed("src/new.rs", "old\n", "new\n")
            .build(),
    );
    tree.rebuild(&nested_document());
    assert!(labels(&tree).contains(&"3:crates/z/src/lib.rs#1".to_owned()));
}

#[test]
fn selection_moves_clamps_and_survives_rebuilds_by_path() {
    let mut tree = FileTree::new(&nested_document());
    assert_eq!(
        tree.select_boundary(true).map(|entry| entry.file),
        Some(Some(0))
    );
    assert_eq!(
        tree.move_selection(-2).map(|entry| entry.file),
        Some(Some(4))
    );
    assert_eq!(
        tree.move_selection(-100).map(|entry| entry.path.as_str()),
        Some("crates")
    );
    tree.reveal_file(3);
    tree.rebuild(
        &DocumentBuilder::new()
            .changed("src/main.rs", "old\n", "new\n")
            .build(),
    );
    assert_eq!(tree.selected().map(|entry| entry.file), Some(Some(0)));
}

#[test]
fn revealing_expands_ancestors_but_following_selects_the_nearest_visible_one() {
    let mut tree = FileTree::new(&nested_document());
    tree.set_expanded("crates", false);
    tree.set_expanded("crates/z", false);
    tree.follow_file(1);
    assert_eq!(
        tree.selected().map(|entry| entry.path.as_str()),
        Some("crates")
    );
    tree.reveal_file(1);
    assert_eq!(tree.selected().map(|entry| entry.file), Some(Some(1)));
    assert_eq!(tree.selected_position(), Some(3));
}

#[test]
fn directories_aggregate_the_stage_state_of_their_files() {
    let mut document = (*nested_document()).clone();
    document.files[3].staged = StageState::Staged;
    assert_eq!(
        FileTree::stage_state_for(&document, "src"),
        StageState::PartiallyStaged
    );
    assert_eq!(
        FileTree::stage_state_for(&document, "src/main.rs"),
        StageState::Staged
    );
    assert_eq!(
        FileTree::paths_for(&document, "src")
            .iter()
            .map(RepoPath::as_str)
            .collect::<Vec<_>>(),
        ["src/main.rs", "src/lib.rs"]
    );
}

fn nested_document() -> Arc<DiffDocument> {
    DocumentBuilder::new()
        .changed("README.md", "old\n", "new\n")
        .changed("crates/z/src/lib.rs", "old\n", "new\n")
        .changed("crates/a.rs", "old\n", "new\n")
        .changed("src/main.rs", "old\n", "new\n")
        .changed("src/lib.rs", "old\n", "new\n")
        .build()
}

fn labels(tree: &FileTree) -> Vec<String> {
    tree.entries()
        .iter()
        .map(|entry| match entry.file {
            Some(file) => format!("{}:{}#{file}", entry.depth, entry.path),
            None => format!(
                "{}:{}/{}",
                entry.depth,
                entry.path,
                if entry.expanded { "" } else { "-" }
            ),
        })
        .collect()
}
