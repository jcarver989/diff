use crate::{
    DiffDocument, DiffSide, DiffSnapshot, FileDiff, FileStatus, Hunk, PatchLine, RepoPath,
    SourceDocument, SourceKey, SourceUnavailable, StageState,
};
use std::{collections::HashMap, sync::Arc};

/// A document fixture and its eager immutable source snapshot.
#[derive(Debug, Clone)]
pub struct DocumentFixture {
    pub document: Arc<DiffDocument>,
    sources: HashMap<SourceKey, Result<Arc<SourceDocument>, SourceUnavailable>>,
}

impl DocumentFixture {
    #[must_use]
    pub fn snapshot(&self) -> DiffSnapshot {
        DiffSnapshot::from_parts(self.document.clone(), self.sources.clone())
    }
}

#[derive(Debug, Clone)]
pub struct DocumentBuilder {
    repo_root: String,
    files: Vec<FileDiff>,
    sources: HashMap<SourceKey, Result<Arc<str>, SourceUnavailable>>,
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
            sources: HashMap::new(),
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
    pub fn changed(mut self, path: &str, old: &str, new: &str) -> Self {
        let file = FileDiff::from_texts(path, old, new).expect("valid fixture path");
        let review_path = file.path.clone();
        self.sources.insert(
            SourceKey::new(review_path.clone(), DiffSide::Old),
            if file.status == FileStatus::Added {
                Err(SourceUnavailable::Absent)
            } else {
                Ok(Arc::from(old))
            },
        );
        self.sources.insert(
            SourceKey::new(review_path, DiffSide::New),
            if file.status == FileStatus::Deleted {
                Err(SourceUnavailable::Absent)
            } else {
                Ok(Arc::from(new))
            },
        );
        self.file(file)
    }

    /// Adds a changed fixture whose patch contains only the requested source-line window.
    /// Complete old and new text remain available through the fixture source host.
    ///
    /// # Panics
    /// Panics when the path is invalid or the generated diff has no changed hunk.
    #[must_use]
    pub fn changed_with_hunk_window(
        mut self,
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
        let review_path = file.path.clone();
        self.sources.insert(
            SourceKey::new(review_path.clone(), DiffSide::Old),
            Ok(Arc::from(old)),
        );
        self.sources.insert(
            SourceKey::new(review_path, DiffSide::New),
            Ok(Arc::from(new)),
        );
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

    /// Attaches an exact source result without changing patch metadata.
    #[must_use]
    pub fn source(
        mut self,
        path: &str,
        side: DiffSide,
        result: Result<impl Into<Arc<str>>, SourceUnavailable>,
    ) -> Self {
        self.sources.insert(
            SourceKey::new(fixture_path(path), side),
            result.map(Into::into),
        );
        self
    }

    #[must_use]
    pub fn build(self) -> Arc<DiffDocument> {
        self.build_fixture().document
    }

    #[must_use]
    pub fn build_fixture(self) -> DocumentFixture {
        let sources = self
            .sources
            .into_iter()
            .map(|(key, source)| {
                let source =
                    source.and_then(|text| SourceDocument::new(text.as_ref()).map(Arc::new));
                (key, source)
            })
            .collect();
        DocumentFixture {
            document: Arc::new(DiffDocument {
                repo_root: self.repo_root,
                files: self.files,
            }),
            sources,
        }
    }
}

fn fixture_path(path: &str) -> RepoPath {
    RepoPath::new(path).expect("valid fixture path")
}
