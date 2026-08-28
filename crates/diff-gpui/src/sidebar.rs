use crate::{DiffViewer, style::color};
use diff_core::{DiffDocument, FileStatus, StageState};
use gpui::{ClickEvent, Context, Div, Empty, Role, Stateful, div, prelude::*, px};
use std::collections::{BTreeMap, HashSet};

const HEADER_HEIGHT: f32 = 52.0;
const INDENT_WIDTH: f32 = 16.0;
const DISCLOSURE_WIDTH: f32 = 14.0;
const RESIZE_HANDLE_WIDTH: f32 = 8.0;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SidebarResizeDrag;

#[derive(Debug, Default)]
pub(crate) struct SidebarTree {
    roots: Vec<TreeNode>,
    collapsed: HashSet<String>,
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

#[derive(Debug, Clone, Copy)]
enum VisibleNode<'a> {
    Directory {
        directory: &'a DirectoryNode,
        depth: u16,
        expanded: bool,
    },
    File {
        file: &'a FileNode,
        depth: u16,
    },
}

impl SidebarTree {
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
        let mut directory_paths = HashSet::new();
        collect_directory_paths(&self.roots, &mut directory_paths);
        self.collapsed
            .retain(|path| directory_paths.contains(path.as_str()));
    }

    pub(crate) fn toggle(&mut self, path: &str) {
        if !self.collapsed.remove(path) {
            self.collapsed.insert(path.to_owned());
        }
    }

    pub(crate) fn expand_file(&mut self, document: &DiffDocument, file_index: usize) {
        let Some(file) = document.files.get(file_index) else {
            return;
        };
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
            self.collapsed.remove(&path);
        }
    }

    fn visible_nodes(&self) -> Vec<VisibleNode<'_>> {
        let mut visible = Vec::new();
        collect_visible_nodes(&self.roots, 0, &self.collapsed, &mut visible);
        visible
    }

    fn file_order(&self) -> Vec<usize> {
        let mut indices = Vec::new();
        collect_file_indices(&self.roots, &mut indices);
        indices
    }

    pub(crate) fn offset_file(&self, current: usize, delta: isize) -> Option<usize> {
        let order = self.file_order();
        let position = order.iter().position(|index| *index == current)?;
        let target = if delta.is_negative() {
            position.saturating_sub(delta.unsigned_abs())
        } else {
            position
                .saturating_add(delta.unsigned_abs())
                .min(order.len().saturating_sub(1))
        };
        order.get(target).copied()
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
        let children = build_nodes(child, &path);
        nodes.push(TreeNode::Directory(DirectoryNode {
            path,
            name,
            children,
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

fn collect_visible_nodes<'a>(
    nodes: &'a [TreeNode],
    depth: u16,
    collapsed: &HashSet<String>,
    visible: &mut Vec<VisibleNode<'a>>,
) {
    for node in nodes {
        match node {
            TreeNode::Directory(directory) => {
                let expanded = !collapsed.contains(&directory.path);
                visible.push(VisibleNode::Directory {
                    directory,
                    depth,
                    expanded,
                });
                if expanded {
                    collect_visible_nodes(&directory.children, depth + 1, collapsed, visible);
                }
            }
            TreeNode::File(file) => visible.push(VisibleNode::File { file, depth }),
        }
    }
}

fn collect_file_indices(nodes: &[TreeNode], indices: &mut Vec<usize>) {
    for node in nodes {
        match node {
            TreeNode::Directory(directory) => collect_file_indices(&directory.children, indices),
            TreeNode::File(file) => indices.push(file.index),
        }
    }
}

fn sidebar_row(depth: u16, height: f32) -> Div {
    div()
        .h(px(height))
        .flex()
        .items_center()
        .gap_2()
        .pr_3()
        .pl(px(12.0 + f32::from(depth) * INDENT_WIDTH))
        .cursor_pointer()
}

impl DiffViewer {
    pub(crate) fn render_sidebar(&self, cx: &mut Context<Self>) -> Div {
        let palette = self.theme().palette();
        let mut rows = div()
            .id("diff-files")
            .role(Role::Tree)
            .flex_1()
            .overflow_y_scroll()
            .py_2();

        for node in self.sidebar_tree.visible_nodes() {
            match node {
                VisibleNode::Directory {
                    directory,
                    depth,
                    expanded,
                } => rows = rows.child(self.render_directory_row(directory, depth, expanded, cx)),
                VisibleNode::File { file, depth } => {
                    if let Some(row) = self.render_file_row(file, depth, cx) {
                        rows = rows.child(row);
                    }
                }
            }
        }

        div()
            .w(px(self.sidebar_width()))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(HEADER_HEIGHT))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .px_4()
                    .border_b_1()
                    .border_color(color(palette.border))
                    .text_size(px(self.heading_font_size()))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(format!("CHANGED FILES  {}", self.document().files.len())),
            )
            .child(rows)
    }

    pub(crate) fn render_sidebar_resize_handle(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let palette = self.theme().palette();
        div()
            .id("sidebar-resize-container")
            .relative()
            .h_full()
            .w(px(1.0))
            .flex_shrink_0()
            .bg(color(palette.border))
            .child(
                div()
                    .id("sidebar-resize-handle")
                    .absolute()
                    .left(px(-RESIZE_HANDLE_WIDTH / 2.0))
                    .w(px(RESIZE_HANDLE_WIDTH))
                    .h_full()
                    .cursor_col_resize()
                    .block_mouse_except_scroll()
                    .hover(|handle| handle.bg(color(palette.accent)))
                    .on_click(cx.listener(|viewer, event: &ClickEvent, _, cx| {
                        if event.click_count() >= 2 {
                            viewer.reset_sidebar_width(cx);
                        }
                        cx.stop_propagation();
                    }))
                    .on_drag(SidebarResizeDrag, |_, _, _, cx| cx.new(|_| Empty)),
            )
    }

    fn render_directory_row(
        &self,
        directory: &DirectoryNode,
        depth: u16,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let palette = self.theme().palette();
        let path = directory.path.clone();
        sidebar_row(depth, self.sidebar_row_height())
            .id(format!("diff-directory:{path}"))
            .role(Role::TreeItem)
            .aria_label(directory.name.clone())
            .aria_level(usize::from(depth) + 1)
            .aria_expanded(expanded)
            .text_color(color(palette.muted))
            .hover(|row| row.bg(color(palette.selection)))
            .on_click(cx.listener(move |viewer, _, _, cx| {
                viewer.toggle_directory(&path, cx);
            }))
            .child(
                div()
                    .w(px(DISCLOSURE_WIDTH))
                    .flex_shrink_0()
                    .text_center()
                    .child(if expanded { "▾" } else { "▸" }),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(directory.name.clone()),
            )
    }

    fn render_file_row(
        &self,
        file: &FileNode,
        depth: u16,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let palette = self.theme().palette();
        let index = file.index;
        let diff = self.document().files.get(index)?;
        let status_color = match diff.status {
            FileStatus::Added | FileStatus::Untracked => palette.addition,
            FileStatus::Deleted => palette.deletion,
            FileStatus::Modified | FileStatus::Renamed | FileStatus::Copied => palette.accent,
        };
        let selected = self.selected_file() == Some(index);
        let stage = stage_marker(diff.staged);
        Some(
            sidebar_row(depth, self.sidebar_row_height())
                .id(("diff-file", index))
                .role(Role::TreeItem)
                .aria_label(diff.path.to_string())
                .aria_level(usize::from(depth) + 1)
                .aria_selected(selected)
                .when(selected, |row| row.bg(color(palette.selection)))
                .hover(|row| row.bg(color(palette.selection)))
                .on_click(cx.listener(move |viewer, _, _, cx| {
                    viewer.select_file(index, cx);
                }))
                .child(div().w(px(DISCLOSURE_WIDTH)).flex_shrink_0())
                .child(
                    div()
                        .w(px(14.0))
                        .flex_shrink_0()
                        .text_color(color(status_color))
                        .child(diff.status.code().to_string()),
                )
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(file.name.clone()),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(self.metadata_font_size()))
                        .text_color(color(palette.muted))
                        .child(format!(
                            "+{} −{} {stage}",
                            diff.additions(),
                            diff.deletions()
                        )),
                ),
        )
    }
}

const fn stage_marker(state: StageState) -> &'static str {
    match state {
        StageState::Staged => "●",
        StageState::PartiallyStaged => "◐",
        StageState::Unstaged => "○",
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

    fn visible_labels(tree: &SidebarTree) -> Vec<String> {
        tree.visible_nodes()
            .into_iter()
            .map(|node| match node {
                VisibleNode::Directory {
                    directory, depth, ..
                } => format!("{depth}:d:{}", directory.path),
                VisibleNode::File { file, depth } => {
                    format!("{depth}:f:{}:{}", file.index, file.name)
                }
            })
            .collect()
    }

    #[test]
    fn builds_a_sorted_tree_while_preserving_document_indices() {
        let document = nested_document();
        let tree = SidebarTree::new(&document);

        assert_eq!(
            visible_labels(&tree),
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
        assert_eq!(tree.file_order(), [1, 2, 4, 3, 0]);
    }

    #[test]
    fn collapsing_a_directory_hides_only_its_descendants() {
        let document = nested_document();
        let mut tree = SidebarTree::new(&document);

        tree.toggle("crates");

        assert_eq!(
            visible_labels(&tree),
            [
                "0:d:crates",
                "0:d:src",
                "1:f:4:lib.rs",
                "1:f:3:main.rs",
                "0:f:0:README.md",
            ]
        );
    }

    #[test]
    fn rebuild_retains_existing_collapse_state_and_prunes_removed_paths() {
        let document = nested_document();
        let mut tree = SidebarTree::new(&document);
        tree.toggle("src");
        tree.toggle("crates/z");

        let replacement = DocumentBuilder::new()
            .changed("src/new.rs", "old\n", "new\n")
            .changed("docs/guide.md", "old\n", "new\n")
            .build();
        tree.rebuild(&replacement);

        assert!(tree.collapsed.contains("src"));
        assert!(!tree.collapsed.contains("crates/z"));
        assert!(!tree.collapsed.contains("docs"));
    }

    #[test]
    fn expanding_a_file_opens_all_of_its_ancestors() {
        let document = nested_document();
        let mut tree = SidebarTree::new(&document);
        tree.toggle("crates");
        tree.toggle("crates/z");
        tree.toggle("crates/z/src");

        tree.expand_file(&document, 1);

        assert!(tree.collapsed.is_empty());
    }

    #[test]
    fn offsets_files_in_display_order_and_clamps_at_the_edges() {
        let document = nested_document();
        let tree = SidebarTree::new(&document);

        assert_eq!(tree.offset_file(1, -1), Some(1));
        assert_eq!(tree.offset_file(1, 1), Some(2));
        assert_eq!(tree.offset_file(3, 1), Some(0));
        assert_eq!(tree.offset_file(0, 1), Some(0));
    }
}
