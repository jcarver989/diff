use crate::{
    DiffDocument, DiffSide, FileDiff, FileStatus, Hunk, PatchLine, RepoPath, SourceDocument,
    SourceResult, SourceUnavailable, StageState,
};
use std::sync::Arc;

/// Builds complete review documents: every text fixture carries both of its
/// source versions, so sessions built from it can reveal unchanged context.
#[derive(Debug, Clone)]
pub struct DocumentBuilder {
    repo_root: String,
    files: Vec<FileDiff>,
    overrides: Vec<(RepoPath, DiffSide, SourceResult)>,
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
            overrides: Vec::new(),
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
        self.changed_with(path, old, new, std::convert::identity)
    }

    /// Adds a changed text fixture after applying `customize` to its valid defaults.
    /// Both complete source versions travel with the file.
    ///
    /// This is the Rust equivalent of building a factory value with overrides. Struct
    /// update syntax keeps tests focused on the fields that matter:
    ///
    /// ```
    /// use clankerdiff_core::{StageState, testing::DocumentBuilder};
    ///
    /// let document = DocumentBuilder::new()
    ///     .changed_with("src/lib.rs", "old\n", "new\n", |file| clankerdiff_core::FileDiff {
    ///         staged: StageState::Staged,
    ///         ..file
    ///     })
    ///     .build();
    /// assert_eq!(document.files[0].staged, StageState::Staged);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics when `path` is not a valid repository-relative fixture path.
    #[must_use]
    pub fn changed_with(
        self,
        path: &str,
        old: &str,
        new: &str,
        customize: impl FnOnce(FileDiff) -> FileDiff,
    ) -> Self {
        let file = customize(FileDiff::from_texts(path, old, new).expect("valid fixture path"));
        self.file(file)
    }

    /// Adds a deleted text fixture.
    #[must_use]
    pub fn deleted(self, path: &str, old: &str) -> Self {
        self.changed(path, old, "")
    }

    /// Adds an untracked text fixture.
    #[must_use]
    pub fn untracked(self, path: &str, new: &str) -> Self {
        self.changed_with(path, "", new, |file| FileDiff {
            status: FileStatus::Untracked,
            ..file
        })
    }

    /// Adds a text fixture with the requested staging state.
    #[must_use]
    pub fn changed_staged(self, path: &str, old: &str, new: &str, staged: StageState) -> Self {
        self.changed_with(path, old, new, |file| FileDiff { staged, ..file })
    }

    /// Adds a renamed text fixture.
    #[must_use]
    pub fn renamed(self, old_path: &str, new_path: &str, old: &str, new: &str) -> Self {
        let old_path = fixture_path(old_path);
        self.changed_with(new_path, old, new, |file| FileDiff {
            old_path: Some(old_path),
            status: FileStatus::Renamed,
            ..file
        })
    }

    /// Adds a copied text fixture.
    #[must_use]
    pub fn copied(self, old_path: &str, new_path: &str, old: &str, new: &str) -> Self {
        let old_path = fixture_path(old_path);
        self.changed_with(new_path, old, new, |file| FileDiff {
            old_path: Some(old_path),
            status: FileStatus::Copied,
            ..file
        })
    }

    /// Adds a changed fixture whose patch contains only the requested source-line window.
    /// Complete old and new text remain attached to the file.
    ///
    /// # Panics
    /// Panics when the path is invalid or the generated diff has no changed hunk.
    #[must_use]
    pub fn changed_with_hunk_window(
        self,
        path: &str,
        old: &str,
        new: &str,
        window: std::ops::RangeInclusive<usize>,
    ) -> Self {
        let mut file = FileDiff::from_texts(path, old, new).expect("valid fixture path");
        let mut hunk = file.hunks.remove(0);
        hunk.lines.retain(|line| {
            line.old_line_no
                .is_some_and(|number| window.contains(&number))
                || line
                    .new_line_no
                    .is_some_and(|number| window.contains(&number))
        });
        let start = *window.start();
        let count = window.end().saturating_sub(start).saturating_add(1);
        hunk.old_start = start;
        hunk.old_count = count;
        hunk.new_start = start;
        hunk.new_count = count;
        hunk.header = format!("@@ -{start},{count} +{start},{count} @@");
        file.hunks = vec![hunk];
        self.file(file)
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
            old_source: Err(SourceUnavailable::Binary),
            new_source: Err(SourceUnavailable::Binary),
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
            old_source: FileDiff::uncaptured_source(FileStatus::Added, DiffSide::Old),
            new_source: FileDiff::uncaptured_source(FileStatus::Added, DiffSide::New),
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

    /// Overrides one side's source result without changing patch metadata.
    /// Applied when the document is built, so it may precede or follow the file.
    #[must_use]
    pub fn source(
        mut self,
        path: &str,
        side: DiffSide,
        result: Result<impl AsRef<str>, SourceUnavailable>,
    ) -> Self {
        let result = result.and_then(|text| SourceDocument::new(text).map(Arc::new));
        self.overrides.push((fixture_path(path), side, result));
        self
    }

    /// Builds the complete document.
    ///
    /// # Panics
    ///
    /// Panics when a source override names a path that was never added.
    #[must_use]
    pub fn build(self) -> Arc<DiffDocument> {
        let mut files = self.files;
        for (path, side, result) in self.overrides {
            let file = files
                .iter_mut()
                .find(|file| file.path == path)
                .unwrap_or_else(|| panic!("source override for unknown fixture path {path}"));
            match side {
                DiffSide::Old => file.old_source = result,
                DiffSide::New => file.new_source = result,
            }
        }
        Arc::new(DiffDocument {
            repo_root: self.repo_root,
            files,
        })
    }
}

/// Returns a valid one-file diff with useful defaults for struct update syntax.
///
/// Prefer [`DocumentBuilder::changed_with`] for documents that a session will project.
/// Use this factory for low-level model tests that intentionally construct unusual
/// combinations of public fields.
///
/// # Panics
///
/// Panics if the built-in default fixture path becomes invalid.
#[must_use]
pub fn file_diff() -> FileDiff {
    FileDiff::from_texts("src/lib.rs", "old\n", "new\n").expect("valid default fixture")
}

/// Returns a valid hunk with one removed and one added line for struct update syntax.
///
/// # Panics
///
/// Panics if [`file_diff`] no longer produces a changed hunk.
#[must_use]
pub fn hunk() -> Hunk {
    file_diff().hunks.remove(0)
}

/// Returns a valid added patch line for struct update syntax.
#[must_use]
pub fn patch_line() -> PatchLine {
    PatchLine::added("new", 1)
}

/// Returns a valid diff document for struct update syntax.
///
/// # Panics
///
/// Panics if the built-in [`file_diff`] defaults become invalid.
#[must_use]
pub fn diff_document() -> DiffDocument {
    DiffDocument {
        repo_root: "/repo".to_owned(),
        files: vec![file_diff()],
    }
}

fn fixture_path(path: &str) -> RepoPath {
    RepoPath::new(path).expect("valid fixture path")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_with_applies_struct_update_overrides_and_preserves_sources() {
        let document = DocumentBuilder::new()
            .changed_with("src/lib.rs", "old\n", "new\n", |file| FileDiff {
                staged: StageState::Staged,
                ..file
            })
            .source(
                "src/lib.rs",
                DiffSide::New,
                Err::<&str, _>(SourceUnavailable::Binary),
            )
            .build();

        let file = &document.files[0];
        assert_eq!(file.staged, StageState::Staged);
        assert_eq!(file.source_document(DiffSide::Old).unwrap().text(), "old\n");
        assert_eq!(
            file.source_unavailable(DiffSide::New),
            Some(&SourceUnavailable::Binary)
        );
    }

    #[test]
    fn low_level_factories_support_struct_update_syntax() {
        let file = FileDiff {
            binary: true,
            hunks: Vec::new(),
            ..file_diff()
        };
        let document = DiffDocument {
            files: vec![file],
            ..diff_document()
        };

        assert!(document.files[0].binary);
    }

    #[test]
    fn common_file_kinds_have_consistent_status_and_sources() {
        let document = DocumentBuilder::new()
            .deleted("deleted.rs", "old\n")
            .untracked("new.rs", "new\n")
            .renamed("old.rs", "renamed.rs", "old\n", "new\n")
            .copied("source.rs", "copy.rs", "old\n", "new\n")
            .build();
        let text = |index: usize, side| {
            document.files[index]
                .source_document(side)
                .expect("fixture source")
                .text()
                .to_owned()
        };

        assert_eq!(document.files[0].status, FileStatus::Deleted);
        assert_eq!(document.files[1].status, FileStatus::Untracked);
        assert_eq!(document.files[2].status, FileStatus::Renamed);
        assert_eq!(document.files[3].status, FileStatus::Copied);
        assert_eq!(
            document.files[0].source_unavailable(DiffSide::New),
            Some(&SourceUnavailable::Absent)
        );
        assert_eq!(
            document.files[1].source_unavailable(DiffSide::Old),
            Some(&SourceUnavailable::Absent)
        );
        assert_eq!(text(1, DiffSide::New), "new\n");
        assert_eq!(text(2, DiffSide::Old), "old\n");
        assert_eq!(text(2, DiffSide::New), "new\n");
        assert_eq!(text(3, DiffSide::Old), "old\n");
        assert_eq!(text(3, DiffSide::New), "new\n");
    }
}
