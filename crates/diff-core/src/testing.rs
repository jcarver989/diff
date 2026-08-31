use crate::{DiffDocument, FileDiff, FileStatus, Hunk, PatchLine, RepoPath, StageState};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct DocumentBuilder {
    repo_root: String,
    files: Vec<FileDiff>,
}

impl Default for DocumentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            repo_root: "/repo".to_owned(),
            files: Vec::new(),
        }
    }

    #[must_use]
    pub fn repo_root(mut self, repo_root: impl Into<String>) -> Self {
        self.repo_root = repo_root.into();
        self
    }

    /// Adds a changed text fixture.
    ///
    /// # Panics
    ///
    /// Panics when `path` is not a valid repository-relative fixture path.
    #[must_use]
    pub fn changed(self, path: &str, old: &str, new: &str) -> Self {
        self.file(FileDiff::from_texts(path, old, new).expect("valid fixture path"))
    }

    #[must_use]
    pub fn added(self, path: &str, new: &str) -> Self {
        self.changed(path, "", new)
    }

    #[must_use]
    pub fn binary(self, path: &str) -> Self {
        let path = fixture_path(path);
        self.file(FileDiff {
            old_path: Some(path.clone()),
            path,
            status: FileStatus::Modified,
            staged: StageState::Unstaged,
            hunks: Vec::new(),
            binary: true,
            mode: None,
            no_newline_at_end: false,
            omitted_bytes: None,
        })
    }

    #[must_use]
    pub fn generated(self, path: &str, lines: usize) -> Self {
        let stem = path.rsplit('/').next().unwrap_or(path).replace('.', "_");
        let patch_lines: Vec<PatchLine> = (1..=lines)
            .map(|line| PatchLine::added(format!("let {stem}_value_{line} = {line};"), line))
            .collect();
        let path = fixture_path(path);
        self.file(FileDiff {
            old_path: None,
            path,
            status: FileStatus::Added,
            staged: StageState::Unstaged,
            hunks: vec![Hunk {
                header: format!("@@ -0,0 +1,{lines} @@"),
                function_context: None,
                old_start: 0,
                old_count: 0,
                new_start: 1,
                new_count: lines,
                lines: patch_lines,
            }],
            binary: false,
            mode: None,
            no_newline_at_end: false,
            omitted_bytes: None,
        })
    }

    #[must_use]
    pub fn generated_files(mut self, count: usize, lines: usize) -> Self {
        for index in 0..count {
            self = self.generated(&format!("src/file_{index:02}.rs"), lines);
        }
        self
    }

    #[must_use]
    pub fn file(mut self, file: FileDiff) -> Self {
        self.files.push(file);
        self
    }

    #[must_use]
    pub fn build(self) -> Arc<DiffDocument> {
        Arc::new(DiffDocument {
            repo_root: self.repo_root,
            files: self.files,
        })
    }
}

fn fixture_path(path: &str) -> RepoPath {
    RepoPath::new(path).expect("valid fixture path")
}
