use crate::{DiffDocument, FileDiff, FileStatus, LineStat, RepoPath, RepositoryAction, StageState};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTreeEntry {
    pub path: String,
    pub name: String,
    pub depth: usize,
    pub file: Option<usize>,
    pub expanded: bool,
    pub stage: StageState,
    pub stat: LineStat,
}

#[derive(Debug, Default)]
pub struct FileTree {
    all: Vec<FileTreeEntry>,
    collapsed: HashSet<String>,
    visible: Vec<FileTreeEntry>,
    selected: Option<String>,
}

#[must_use]
pub fn tree_order(document: &DiffDocument) -> Vec<usize> {
    let mut order = (0..document.files.len()).collect::<Vec<_>>();
    order.sort_by(|&left, &right| {
        tree_key(&document.files[left].path).cmp(tree_key(&document.files[right].path)).then(left.cmp(&right))
    });
    order
}

fn tree_key(path: &RepoPath) -> impl Iterator<Item = (bool, &str)> {
    let mut components = path.as_str().split('/').peekable();
    std::iter::from_fn(move || {
        let component = components.next()?;
        Some((components.peek().is_none(), component))
    })
}

impl FileTree {
    #[must_use]
    pub fn new(document: &DiffDocument) -> Self {
        let mut tree = Self::default();
        tree.rebuild(document);
        tree
    }

    pub fn rebuild(&mut self, document: &DiffDocument) {
        self.all.clear();
        let mut open: Vec<&str> = Vec::new();
        let mut ancestors: Vec<usize> = Vec::new();
        for index in tree_order(document) {
            let file = &document.files[index];
            let stat = file.line_stat();
            let directories = file.path.directory().map_or_else(Vec::new, |directory| directory.split('/').collect());
            let shared = open.iter().zip(&directories).take_while(|(left, right)| left == right).count();
            ancestors.truncate(shared);
            for depth in shared..directories.len() {
                ancestors.push(self.all.len());
                self.all.push(FileTreeEntry {
                    path: directories[..=depth].join("/"),
                    name: directories[depth].to_owned(),
                    depth,
                    file: None,
                    expanded: true,
                    stage: file.staged,
                    stat: LineStat::default(),
                });
            }
            for &ancestor in &ancestors {
                let directory = &mut self.all[ancestor];
                directory.stage = merge_stage(directory.stage, file.staged);
                directory.stat += stat;
            }
            self.all.push(FileTreeEntry {
                path: file.path.to_string(),
                name: file.path.file_name().to_owned(),
                depth: directories.len(),
                file: Some(index),
                expanded: true,
                stage: file.staged,
                stat,
            });
            open = directories;
        }
        let all = &self.all;
        let exists = |path: &String| all.iter().any(|entry| entry.path == *path);
        self.collapsed.retain(exists);
        self.selected = self.selected.take().filter(exists);
        self.refresh();
    }

    #[must_use]
    pub fn entries(&self) -> &[FileTreeEntry] {
        &self.visible
    }

    #[must_use]
    pub fn selected_position(&self) -> Option<usize> {
        let selected = self.selected.as_ref()?;
        self.visible.iter().position(|entry| entry.path == *selected)
    }

    #[must_use]
    pub fn selected(&self) -> Option<&FileTreeEntry> {
        self.visible.get(self.selected_position()?)
    }

    pub fn select(&mut self, position: usize) -> Option<&FileTreeEntry> {
        let entry = self.visible.get(position)?;
        self.selected = Some(entry.path.clone());
        Some(entry)
    }

    pub fn move_selection(&mut self, delta: isize) -> Option<&FileTreeEntry> {
        let current = self.selected_position().unwrap_or(0);
        let target = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta.unsigned_abs()).min(self.visible.len().saturating_sub(1))
        };
        self.select(target)
    }

    pub fn select_boundary(&mut self, end: bool) -> Option<&FileTreeEntry> {
        self.select(if end { self.visible.len().saturating_sub(1) } else { 0 })
    }

    pub fn set_expanded(&mut self, path: &str, expanded: bool) {
        let changed = if expanded { self.collapsed.remove(path) } else { self.collapsed.insert(path.to_owned()) };
        if changed {
            self.refresh();
        }
    }

    pub fn toggle(&mut self, path: &str) {
        self.set_expanded(path, self.collapsed.contains(path));
    }

    pub fn reveal_file(&mut self, file: usize) {
        let Some(path) = self.file_path(file) else {
            return;
        };
        let mut changed = false;
        for (end, _) in path.match_indices('/') {
            changed |= self.collapsed.remove(&path[..end]);
        }
        if changed {
            self.refresh();
        }
        self.selected = Some(path);
    }

    pub fn follow_file(&mut self, file: usize) {
        let Some(path) = self.file_path(file) else {
            return;
        };
        let visible = std::iter::once(path.as_str())
            .chain(path.match_indices('/').rev().map(|(end, _)| &path[..end]))
            .find(|candidate| self.visible.iter().any(|entry| entry.path == *candidate));
        if let Some(visible) = visible {
            self.selected = Some(visible.to_owned());
        }
    }

    #[must_use]
    pub fn paths_for(document: &DiffDocument, path: &str) -> Vec<RepoPath> {
        files_under(document, path).map(|file| file.path.clone()).collect()
    }

    #[must_use]
    pub fn toggle_stage_action(&self, document: &DiffDocument, path: &str) -> Option<RepositoryAction> {
        let unstage = self.all.iter().find(|entry| entry.path == path)?.stage == StageState::Staged;
        let mut paths = Vec::new();
        let mut seen = HashSet::new();
        for file in files_under(document, path) {
            if !unstage && file.staged == StageState::Staged {
                continue;
            }
            let old_path = file.old_path.as_ref().filter(|_| file.status == FileStatus::Renamed);
            for path in std::iter::once(&file.path).chain(old_path) {
                if seen.insert(path.clone()) {
                    paths.push(path.clone());
                }
            }
        }
        if paths.is_empty() {
            None
        } else if unstage {
            Some(RepositoryAction::UnstagePaths(paths))
        } else {
            Some(RepositoryAction::StagePaths(paths))
        }
    }

    fn file_path(&self, file: usize) -> Option<String> {
        self.all.iter().find(|entry| entry.file == Some(file)).map(|entry| entry.path.clone())
    }

    fn refresh(&mut self) {
        let mut hidden_below = None;
        self.visible = self
            .all
            .iter()
            .filter(|entry| {
                if hidden_below.is_some_and(|depth| entry.depth > depth) {
                    return false;
                }
                hidden_below = self.collapsed.contains(&entry.path).then_some(entry.depth);
                true
            })
            .map(|entry| FileTreeEntry { expanded: !self.collapsed.contains(&entry.path), ..entry.clone() })
            .collect();
    }
}

fn merge_stage(left: StageState, right: StageState) -> StageState {
    if left == right { left } else { StageState::PartiallyStaged }
}

fn files_under<'a>(document: &'a DiffDocument, path: &'a str) -> impl Iterator<Item = &'a FileDiff> {
    document.files.iter().filter(move |file| {
        file.path.as_str().strip_prefix(path).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}
