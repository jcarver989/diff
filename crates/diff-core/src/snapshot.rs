//! Immutable review snapshots: patch metadata and complete source documents.
use crate::{
    DiffDocument, DiffError, DiffSide, FileDiff, FileStatus, RepoPath, RepoPathError,
    SourceDocument, SourceKey, SourceUnavailable,
};
use std::{collections::HashMap, sync::Arc};

/// Immutable per-side source results keyed by review path.
pub type SnapshotSources = HashMap<SourceKey, Result<Arc<SourceDocument>, SourceUnavailable>>;

/// A renderer-independent immutable review snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffSnapshot {
    document: Arc<DiffDocument>,
    sources: SnapshotSources,
}

impl DiffSnapshot {
    #[must_use]
    pub fn new(document: DiffDocument, sources: SnapshotSources) -> Self {
        Self {
            document: Arc::new(document),
            sources,
        }
    }
    #[must_use]
    pub fn from_parts(document: Arc<DiffDocument>, sources: SnapshotSources) -> Self {
        Self { document, sources }
    }

    /// Builds a complete snapshot from full old/new text pairs, deriving
    /// grouped hunks and retaining both sides for gap expansion and whole-file
    /// syntax context. Hosts holding complete text snapshots use this instead
    /// of assembling a patch-only [`DiffDocument`].
    ///
    /// Sides that exceed the complete-file limits are retained as typed
    /// unavailable reasons and degrade to the patch-only experience.
    ///
    /// # Errors
    /// Returns an error when a path is not a valid repository-relative path.
    pub fn from_texts<'a, T, I>(files: I) -> Result<Self, DiffError>
    where
        T: TryInto<RepoPath>,
        T::Error: Into<RepoPathError>,
        I: IntoIterator<Item = (T, &'a str, &'a str)>,
    {
        let mut documents = Vec::new();
        let mut sources = SnapshotSources::new();
        for (path, old, new) in files {
            let file = FileDiff::from_texts(path, old, new)?;
            let side_source = |text: &str, absent: bool| {
                if absent {
                    Err(SourceUnavailable::Absent)
                } else {
                    SourceDocument::new(text).map(Arc::new)
                }
            };
            sources.insert(
                SourceKey::new(file.path.clone(), DiffSide::Old),
                side_source(old, file.status == FileStatus::Added),
            );
            sources.insert(
                SourceKey::new(file.path.clone(), DiffSide::New),
                side_source(new, file.status == FileStatus::Deleted),
            );
            documents.push(file);
        }
        Ok(Self::new(
            DiffDocument {
                repo_root: String::new(),
                files: documents,
            },
            sources,
        ))
    }

    #[must_use]
    pub const fn document(&self) -> &Arc<DiffDocument> {
        &self.document
    }
    #[must_use]
    pub fn source(
        &self,
        key: &SourceKey,
    ) -> Option<&Result<Arc<SourceDocument>, SourceUnavailable>> {
        self.sources.get(key)
    }

    #[must_use]
    pub const fn sources(&self) -> &SnapshotSources {
        &self.sources
    }

    #[must_use]
    pub fn into_parts(self) -> (Arc<DiffDocument>, SnapshotSources) {
        (self.document, self.sources)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RevealAmount, ReviewSession, RowKind};
    use std::fmt::Write;

    #[test]
    fn from_texts_captures_both_sides_and_derives_grouped_hunks() {
        let old = (1..=80).fold(String::new(), |mut text, line| {
            let _ = writeln!(text, "line {line}");
            text
        });
        let new = old.replace("line 40\n", "changed 40\n");
        let snapshot = DiffSnapshot::from_texts([
            ("src/changed.rs", old.as_str(), new.as_str()),
            ("src/added.rs", "", "fn main() {}\n"),
        ])
        .unwrap();

        let changed = &snapshot.document().files[0];
        assert_eq!(changed.hunks.len(), 1);
        assert!(changed.hunks[0].lines.len() <= 8);
        let key = |path: &str, side| SourceKey::new(RepoPath::new(path).unwrap(), side);
        let old_source = snapshot
            .source(&key("src/changed.rs", DiffSide::Old))
            .unwrap()
            .as_ref()
            .unwrap();
        assert_eq!(old_source.line_count(), 80);
        assert_eq!(
            snapshot
                .source(&key("src/added.rs", DiffSide::Old))
                .unwrap()
                .as_ref()
                .unwrap_err(),
            &SourceUnavailable::Absent
        );

        let mut session = ReviewSession::from_snapshot(snapshot);
        let code_row = (0..session.presentation().row_count())
            .find(|index| {
                session
                    .presentation()
                    .row(*index)
                    .is_some_and(|row| row.kind == RowKind::Code)
            })
            .expect("a code row");
        assert!(session.select_row(code_row));
        assert!(session.reveal_selected_gap(RevealAmount::Step));
        let presentation = session.presentation();
        let expanded = presentation
            .rows(0..presentation.row_count())
            .iter()
            .filter(|row| row.kind == RowKind::ExpandedContext)
            .count();
        assert!(expanded > 0, "captured sides reveal unchanged context");
        let gap = (0..presentation.row_count())
            .find_map(|index| presentation.gap_info(index))
            .expect("a remaining collapsed gap");
        assert_eq!(gap.unavailable, None);
    }
}
