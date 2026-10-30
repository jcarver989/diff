use crate::{DiffDocument, FileDiff, FileStatus, LineStat, RepoPath, RepositoryAction, StageState};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTreeEntry {
    pub path: String,
    pub name: String,
    pub depth: usize,
    pub directory: bool,
    pub file: Option<usize>,
    pub changed: bool,
    pub expanded: bool,
    pub stage: StageState,
    pub stat: LineStat,
}

#[derive(Debug, Default)]
pub struct FileTree {
    repository: Option<Arc<[RepoPath]>>,
    all: Vec<FileTreeEntry>,
    toggled: HashSet<String>,
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

    pub fn set_repository(&mut self, paths: &[RepoPath], document: &DiffDocument) {
        let mut sorted = paths.to_vec();
        sorted.sort_by(|left, right| tree_key(left).cmp(tree_key(right)));
        self.repository = Some(sorted.into());
        self.rebuild(document);
    }

    pub fn rebuild(&mut self, document: &DiffDocument) {
        self.all.clear();
        let mut open: Vec<&str> = Vec::new();
        let mut ancestors: Vec<usize> = Vec::new();
        let repository = self.repository.clone();
        for (path, file) in listed(repository.as_deref(), document) {
            let diff = file.map(|index| &document.files[index]);
            let stat = diff.map(FileDiff::line_stat).unwrap_or_default();
            let stage = diff.map_or(StageState::Unstaged, |diff| diff.staged);
            let directories = path.directory().map_or_else(Vec::new, |directory| directory.split('/').collect());
            let shared = open.iter().zip(&directories).take_while(|(left, right)| left == right).count();
            ancestors.truncate(shared);
            for depth in shared..directories.len() {
                ancestors.push(self.all.len());
                self.all.push(FileTreeEntry {
                    path: directories[..=depth].join("/"),
                    name: directories[depth].to_owned(),
                    depth,
                    directory: true,
                    file: None,
                    changed: false,
                    expanded: true,
                    stage,
                    stat: LineStat::default(),
                });
            }
            if diff.is_some() {
                for &ancestor in &ancestors {
                    let directory = &mut self.all[ancestor];
                    directory.stage = if directory.changed { merge_stage(directory.stage, stage) } else { stage };
                    directory.changed = true;
                    directory.stat += stat;
                }
            }
            self.all.push(FileTreeEntry {
                path: path.to_string(),
                name: path.file_name().to_owned(),
                depth: directories.len(),
                directory: false,
                file,
                changed: file.is_some(),
                expanded: true,
                stage,
                stat,
            });
            open = directories;
        }
        let all = &self.all;
        let directory = |path: &String| all.iter().any(|entry| entry.directory && entry.path == *path);
        self.toggled.retain(directory);
        self.selected = self.selected.take().filter(|path| all.iter().any(|entry| entry.path == *path));
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
        if self.toggle_state(path, expanded) {
            self.refresh();
        }
    }

    pub fn toggle(&mut self, path: &str) {
        self.set_expanded(path, !self.is_expanded(path));
    }

    pub fn reveal_file(&mut self, file: usize) {
        if let Some(path) = self.file_path(file) {
            self.reveal_path(&path);
        }
    }

    pub fn reveal_path(&mut self, path: &str) {
        if !self.all.iter().any(|entry| entry.path == path) {
            return;
        }
        let mut changed = false;
        for (end, _) in path.match_indices('/') {
            changed |= self.toggle_state(&path[..end], true);
        }
        if changed {
            self.refresh();
        }
        self.selected = Some(path.to_owned());
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
        Some(if unstage { RepositoryAction::UnstagePaths(paths) } else { RepositoryAction::StagePaths(paths) })
    }

    fn is_expanded(&self, path: &str) -> bool {
        self.toggled.contains(path) == self.repository.is_some()
    }

    fn toggle_state(&mut self, path: &str, expanded: bool) -> bool {
        if expanded == self.is_expanded(path) {
            return false;
        }
        if !self.toggled.remove(path) {
            self.toggled.insert(path.to_owned());
        }
        true
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
                hidden_below = (entry.directory && !self.is_expanded(&entry.path)).then_some(entry.depth);
                true
            })
            .map(|entry| FileTreeEntry { expanded: !entry.directory || self.is_expanded(&entry.path), ..entry.clone() })
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

fn listed<'a>(repository: Option<&'a [RepoPath]>, document: &'a DiffDocument) -> Vec<(&'a RepoPath, Option<usize>)> {
    let Some(paths) = repository else {
        return tree_order(document).into_iter().map(|index| (&document.files[index].path, Some(index))).collect();
    };
    let mut changed: HashMap<&RepoPath, usize> = document
        .files
        .iter()
        .enumerate()
        .filter(|(_, file)| file.status != FileStatus::Deleted)
        .map(|(index, file)| (&file.path, index))
        .collect();
    let mut listed: Vec<_> = paths.iter().map(|path| (path, changed.remove(path))).collect();
    if !changed.is_empty() {
        listed.extend(changed.into_iter().map(|(path, index)| (path, Some(index))));
        listed.sort_by(|(left, _), (right, _)| tree_key(left).cmp(tree_key(right)));
    }
    listed
}
