use diff_core::{DiffDocument, RepoPath, StageState};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DrawerEntry {
    Directory {
        path: String,
        name: String,
        depth: usize,
        expanded: bool,
    },
    File {
        index: usize,
        name: String,
        depth: usize,
    },
}

#[derive(Debug, Default)]
pub(crate) struct DrawerTree {
    roots: Vec<TreeNode>,
    collapsed: HashSet<String>,
    visible: Vec<DrawerEntry>,
}

#[derive(Debug)]
enum TreeNode {
    Directory(DirectoryNode),
    File(FileNode),
}

#[derive(Debug)]
struct DirectoryNode {
    path: String,
    name: String,
    children: Vec<TreeNode>,
}

#[derive(Debug)]
struct FileNode {
    index: usize,
    name: String,
}

#[derive(Debug, Default)]
struct DirectoryBuilder {
    directories: BTreeMap<String, DirectoryBuilder>,
    files: Vec<FileNode>,
}

impl DrawerTree {
    pub(crate) fn new(document: &DiffDocument) -> Self {
        let mut tree = Self::default();
        tree.rebuild(document);
        tree
    }

    pub(crate) fn rebuild(&mut self, document: &DiffDocument) {
        let mut root = DirectoryBuilder::default();
        for (index, file) in document.files.iter().enumerate() {
            let mut components = file.path.as_str().split('/').peekable();
            let mut directory = &mut root;
            while let Some(component) = components.next() {
                if components.peek().is_none() {
                    directory.files.push(FileNode {
                        index,
                        name: component.to_owned(),
                    });
                } else {
                    directory = directory
                        .directories
                        .entry(component.to_owned())
                        .or_default();
                }
            }
        }

        self.roots = build_nodes(root, "");
        let mut paths = HashSet::new();
        collect_directory_paths(&self.roots, &mut paths);
        self.collapsed.retain(|path| paths.contains(path.as_str()));
        self.refresh_visible();
    }

    pub(crate) fn entries(&self) -> &[DrawerEntry] {
        &self.visible
    }

    pub(crate) fn entry(&self, index: usize) -> Option<&DrawerEntry> {
        self.visible.get(index)
    }

    pub(crate) fn collapse(&mut self, path: &str) {
        if self.collapsed.insert(path.to_owned()) {
            self.refresh_visible();
        }
    }

    pub(crate) fn expand(&mut self, path: &str) {
        if self.collapsed.remove(path) {
            self.refresh_visible();
        }
    }

    pub(crate) fn expand_file(&mut self, document: &DiffDocument, file_index: usize) {
        let Some(file) = document.files.get(file_index) else {
            return;
        };
        let mut changed = false;
        let mut path = String::new();
        let mut components = file.path.as_str().split('/').peekable();
        while let Some(component) = components.next() {
            if components.peek().is_none() {
                break;
            }
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(component);
            changed |= self.collapsed.remove(&path);
        }
        if changed {
            self.refresh_visible();
        }
    }

    pub(crate) fn position_of_file(&self, file_index: usize) -> Option<usize> {
        self.visible.iter().position(
            |entry| matches!(entry, DrawerEntry::File { index, .. } if *index == file_index),
        )
    }

    pub(crate) fn position_of_directory(&self, directory_path: &str) -> Option<usize> {
        self.visible.iter().position(
            |entry| matches!(entry, DrawerEntry::Directory { path, .. } if path == directory_path),
        )
    }

    pub(crate) fn paths_for_entry(document: &DiffDocument, entry: &DrawerEntry) -> Vec<RepoPath> {
        match entry {
            DrawerEntry::File { index, .. } => document
                .files
                .get(*index)
                .map(|file| vec![file.path.clone()])
                .unwrap_or_default(),
            DrawerEntry::Directory { path, .. } => {
                let prefix = format!("{path}/");
                document
                    .files
                    .iter()
                    .filter(|file| file.path.as_str().starts_with(&prefix))
                    .map(|file| file.path.clone())
                    .collect()
            }
        }
    }

    pub(crate) fn stage_state_for_entry(
        document: &DiffDocument,
        entry: &DrawerEntry,
    ) -> StageState {
        match entry {
            DrawerEntry::File { index, .. } => document
                .files
                .get(*index)
                .map_or(StageState::Unstaged, |file| file.staged),
            DrawerEntry::Directory { path, .. } => {
                let prefix = format!("{path}/");
                aggregate_stage_states(
                    document
                        .files
                        .iter()
                        .filter(|file| file.path.as_str().starts_with(&prefix))
                        .map(|file| file.staged),
                )
            }
        }
    }

    fn refresh_visible(&mut self) {
        let mut visible = Vec::new();
        collect_visible(&self.roots, 0, &self.collapsed, &mut visible);
        self.visible = visible;
    }
}

fn aggregate_stage_states(states: impl IntoIterator<Item = StageState>) -> StageState {
    let mut states = states.into_iter();
    let Some(first) = states.next() else {
        return StageState::Unstaged;
    };
    if first == StageState::PartiallyStaged || states.any(|state| state != first) {
        StageState::PartiallyStaged
    } else {
        first
    }
}

fn build_nodes(builder: DirectoryBuilder, parent_path: &str) -> Vec<TreeNode> {
    let mut nodes = Vec::with_capacity(builder.directories.len() + builder.files.len());
    for (name, child) in builder.directories {
        let path = if parent_path.is_empty() {
            name.clone()
        } else {
            format!("{parent_path}/{name}")
        };
        nodes.push(TreeNode::Directory(DirectoryNode {
            children: build_nodes(child, &path),
            path,
            name,
        }));
    }

    let mut files = builder.files;
    files.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.index.cmp(&right.index))
    });
    nodes.extend(files.into_iter().map(TreeNode::File));
    nodes
}

fn collect_directory_paths<'a>(nodes: &'a [TreeNode], paths: &mut HashSet<&'a str>) {
    for node in nodes {
        if let TreeNode::Directory(directory) = node {
            paths.insert(&directory.path);
            collect_directory_paths(&directory.children, paths);
        }
    }
}

fn collect_visible(
    nodes: &[TreeNode],
    depth: usize,
    collapsed: &HashSet<String>,
    visible: &mut Vec<DrawerEntry>,
) {
    for node in nodes {
        match node {
            TreeNode::Directory(directory) => {
                let expanded = !collapsed.contains(&directory.path);
                visible.push(DrawerEntry::Directory {
                    path: directory.path.clone(),
                    name: directory.name.clone(),
                    depth,
                    expanded,
                });
                if expanded {
                    collect_visible(&directory.children, depth + 1, collapsed, visible);
                }
            }
            TreeNode::File(file) => visible.push(DrawerEntry::File {
                index: file.index,
                name: file.name.clone(),
                depth,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diff_core::testing::DocumentBuilder;

    fn nested_document() -> std::sync::Arc<DiffDocument> {
        DocumentBuilder::new()
            .changed("README.md", "old\n", "new\n")
            .changed("crates/z/src/lib.rs", "old\n", "new\n")
            .changed("crates/a.rs", "old\n", "new\n")
            .changed("src/main.rs", "old\n", "new\n")
            .changed("src/lib.rs", "old\n", "new\n")
            .build()
    }

    fn labels(tree: &DrawerTree) -> Vec<String> {
        tree.entries()
            .iter()
            .map(|entry| match entry {
                DrawerEntry::Directory { path, depth, .. } => format!("{depth}:d:{path}"),
                DrawerEntry::File {
                    index, name, depth, ..
                } => format!("{depth}:f:{index}:{name}"),
            })
            .collect()
    }

    #[test]
    fn builds_a_sorted_tree_while_preserving_document_indices() {
        let tree = DrawerTree::new(&nested_document());
        assert_eq!(
            labels(&tree),
            [
                "0:d:crates",
                "1:d:crates/z",
                "2:d:crates/z/src",
                "3:f:1:lib.rs",
                "1:f:2:a.rs",
                "0:d:src",
                "1:f:4:lib.rs",
                "1:f:3:main.rs",
                "0:f:0:README.md",
            ]
        );
    }

    #[test]
    fn collapsing_hides_descendants_and_expanding_restores_them() {
        let mut tree = DrawerTree::new(&nested_document());
        tree.collapse("crates");
        assert_eq!(
            labels(&tree),
            [
                "0:d:crates",
                "0:d:src",
                "1:f:4:lib.rs",
                "1:f:3:main.rs",
                "0:f:0:README.md",
            ]
        );
        tree.expand("crates");
        assert!(labels(&tree).contains(&"3:f:1:lib.rs".to_owned()));
    }

    #[test]
    fn rebuild_retains_valid_collapse_state_and_prunes_removed_paths() {
        let mut tree = DrawerTree::new(&nested_document());
        tree.collapse("src");
        tree.collapse("crates/z");
        let replacement = DocumentBuilder::new()
            .changed("src/new.rs", "old\n", "new\n")
            .changed("docs/guide.md", "old\n", "new\n")
            .build();

        tree.rebuild(&replacement);

        assert!(matches!(
            tree.entry(tree.position_of_directory("src").unwrap()),
            Some(DrawerEntry::Directory {
                expanded: false,
                ..
            })
        ));
        assert!(tree.position_of_directory("crates/z").is_none());
        assert!(matches!(
            tree.entry(tree.position_of_directory("docs").unwrap()),
            Some(DrawerEntry::Directory { expanded: true, .. })
        ));
    }

    #[test]
    fn directories_aggregate_and_include_collapsed_descendants() {
        let mut document = (*nested_document()).clone();
        document.files[3].staged = StageState::Staged;
        document.files[4].staged = StageState::Unstaged;
        let mut tree = DrawerTree::new(&document);
        let entry = tree
            .entry(tree.position_of_directory("src").unwrap())
            .unwrap()
            .clone();
        tree.collapse("src");

        assert_eq!(
            DrawerTree::stage_state_for_entry(&document, &entry),
            StageState::PartiallyStaged
        );
        assert_eq!(
            DrawerTree::paths_for_entry(&document, &entry)
                .iter()
                .map(RepoPath::as_str)
                .collect::<Vec<_>>(),
            ["src/main.rs", "src/lib.rs"]
        );
    }

    #[test]
    fn expanding_a_file_opens_all_ancestors() {
        let document = nested_document();
        let mut tree = DrawerTree::new(&document);
        tree.collapse("crates");
        tree.collapse("crates/z");
        assert!(tree.position_of_file(1).is_none());

        tree.expand_file(&document, 1);

        assert!(tree.position_of_file(1).is_some());
    }
}
