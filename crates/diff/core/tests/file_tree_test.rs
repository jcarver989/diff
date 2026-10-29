#![cfg(feature = "test-support")]

use clankerdiff_core::{
    DiffDocument, FileTree, FileTreeEntry, LineStat, RepoPath, RepositoryAction, StageState, testing::DocumentBuilder,
    tree_order,
};
use std::{error::Error, sync::Arc};

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
    assert_eq!(labels(&tree), ["0:crates/-", "0:src/", "1:src/lib.rs#4", "1:src/main.rs#3", "0:README.md#0"]);
    tree.toggle("crates");
    assert!(labels(&tree).contains(&"1:crates/z/-".to_owned()));
    tree.rebuild(&DocumentBuilder::new().changed("src/new.rs", "old\n", "new\n").build());
    tree.rebuild(&nested_document());
    assert!(labels(&tree).contains(&"3:crates/z/src/lib.rs#1".to_owned()));
}

#[test]
fn selection_moves_clamps_and_survives_rebuilds_by_path() {
    let mut tree = FileTree::new(&nested_document());
    assert_eq!(tree.select_boundary(true).map(|entry| entry.file), Some(Some(0)));
    assert_eq!(tree.move_selection(-2).map(|entry| entry.file), Some(Some(4)));
    assert_eq!(tree.move_selection(-100).map(|entry| entry.path.as_str()), Some("crates"));
    tree.reveal_file(3);
    tree.rebuild(&DocumentBuilder::new().changed("src/main.rs", "old\n", "new\n").build());
    assert_eq!(tree.selected().map(|entry| entry.file), Some(Some(0)));
}

#[test]
fn revealing_expands_ancestors_but_following_selects_the_nearest_visible_one() {
    let mut tree = FileTree::new(&nested_document());
    tree.set_expanded("crates", false);
    tree.set_expanded("crates/z", false);
    tree.follow_file(1);
    assert_eq!(tree.selected().map(|entry| entry.path.as_str()), Some("crates"));
    tree.reveal_file(1);
    assert_eq!(tree.selected().map(|entry| entry.file), Some(Some(1)));
    assert_eq!(tree.selected_position(), Some(3));
}

#[test]
fn a_partially_staged_directory_lists_its_files_and_stages_the_rest() {
    let mut document = (*nested_document()).clone();
    document.files[3].staged = StageState::Staged;
    let tree = FileTree::new(&document);
    assert_eq!(
        FileTree::paths_for(&document, "src").iter().map(RepoPath::as_str).collect::<Vec<_>>(),
        ["src/main.rs", "src/lib.rs"]
    );
    assert_eq!(
        tree.toggle_stage_action(&document, "src"),
        Some(RepositoryAction::StagePaths(vec![document.files[4].path.clone()]))
    );
}

#[test]
fn entries_carry_the_stage_state_and_line_counts_of_the_files_beneath_them() {
    let document = DocumentBuilder::new()
        .changed_staged("src/staged.rs", "a\nb\n", "c\n", StageState::Staged)
        .changed("src/edit.rs", "old\n", "new\nmore\n")
        .changed_staged("docs/guide.md", "old\n", "new\n", StageState::Staged)
        .build();
    let tree = FileTree::new(&document);
    let summary = |path: &str| entry(&tree, path).map(|entry| (entry.stage, entry.stat));
    assert_eq!(summary("src/staged.rs"), Some((StageState::Staged, line_stat(1, 2, 1))));
    assert_eq!(summary("src/edit.rs"), Some((StageState::Unstaged, line_stat(2, 1, 1))));
    assert_eq!(summary("src"), Some((StageState::PartiallyStaged, line_stat(3, 3, 2))));
    assert_eq!(summary("docs"), Some((StageState::Staged, line_stat(1, 1, 1))));
}

#[test]
fn staging_a_directory_skips_staged_files_but_includes_partial_changes() -> Result<(), Box<dyn Error>> {
    let document = DocumentBuilder::new()
        .changed_staged("src/deleted.rs", "old\n", "", StageState::Staged)
        .changed_staged("src/partial.rs", "old\n", "new\n", StageState::PartiallyStaged)
        .changed("src/edit.rs", "old\n", "new\n")
        .changed("src-other/unrelated.rs", "old\n", "new\n")
        .build();
    assert_eq!(
        toggle_stage_action(&document, "src"),
        Some(RepositoryAction::StagePaths(vec![RepoPath::new("src/partial.rs")?, RepoPath::new("src/edit.rs")?,]))
    );
    assert_eq!(toggle_stage_action(&document, "missing"), None);
    Ok(())
}

#[test]
fn toggling_a_rename_includes_both_paths_outside_the_selected_directory() -> Result<(), Box<dyn Error>> {
    let mut document =
        (*DocumentBuilder::new().renamed("old/source.rs", "new/destination.rs", "content\n", "content\n").build())
            .clone();
    let paths = vec![RepoPath::new("new/destination.rs")?, RepoPath::new("old/source.rs")?];
    assert_eq!(toggle_stage_action(&document, "new"), Some(RepositoryAction::StagePaths(paths.clone())));
    document.files[0].staged = StageState::Staged;
    assert_eq!(toggle_stage_action(&document, "new"), Some(RepositoryAction::UnstagePaths(paths)));
    Ok(())
}

#[test]
fn toggling_a_copy_does_not_include_its_source() -> Result<(), Box<dyn Error>> {
    let mut document =
        (*DocumentBuilder::new().copied("old/source.rs", "new/copy.rs", "content\n", "content\n").build()).clone();
    let paths = vec![RepoPath::new("new/copy.rs")?];
    assert_eq!(toggle_stage_action(&document, "new"), Some(RepositoryAction::StagePaths(paths.clone())));
    document.files[0].staged = StageState::Staged;
    assert_eq!(toggle_stage_action(&document, "new"), Some(RepositoryAction::UnstagePaths(paths)));
    Ok(())
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
            None => format!("{}:{}/{}", entry.depth, entry.path, if entry.expanded { "" } else { "-" }),
        })
        .collect()
}

fn entry<'a>(tree: &'a FileTree, path: &str) -> Option<&'a FileTreeEntry> {
    tree.entries().iter().find(|entry| entry.path == path)
}

const fn line_stat(added: usize, deleted: usize, files: usize) -> LineStat {
    LineStat { added, deleted, files }
}

fn toggle_stage_action(document: &DiffDocument, path: &str) -> Option<RepositoryAction> {
    FileTree::new(document).toggle_stage_action(document, path)
}
