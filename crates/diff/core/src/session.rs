use crate::{
    CommitPage, CommitStack, CommitTitle, ContentProjection, DiffDocument, DiffPresentation, DiffReviewCommand,
    DiffReviewEvent, DiffScope, DiffSide, FileDiff, Layout, LineAnchor, PageLabel, PresentationOptions, PresentedCell,
    PresentedRow, RepoPath, Review, ReviewCommand, ReviewTarget, RowId, SourceFocus, SourceLocation, SourcePath,
    TargetLocation, UNCOMMITTED_CHANGES, ViewMode, commit_description, commit_picker::format_age,
};
use navigation::{Navigation, PendingReference};
use std::{ops::Range, sync::Arc, time::Duration};

mod comments;
mod navigation;
mod view;

pub use comments::CommentDraft;
pub use view::{FilesMode, RevealAmount};

#[derive(Debug, Clone)]
struct SelectionBookmark {
    path: RepoPath,
    row: Option<RowId>,
    source: Option<SourceLocation>,
    anchor: Option<LineAnchor>,
    side: DiffSide,
}

#[derive(Debug, Clone)]
struct SourceView {
    target: TargetLocation,
    reference: Option<Arc<DiffDocument>>,
    preview: bool,
    return_to: Option<SelectionBookmark>,
}

fn document_with_reference(document: &DiffDocument, file: FileDiff) -> Arc<DiffDocument> {
    let mut shown = document.clone();
    shown.files.push(file);
    Arc::new(shown)
}

#[derive(Debug, Clone)]
pub struct ReviewSession {
    document: Arc<DiffDocument>,
    presentation: DiffPresentation,
    review: Review,
    scope: DiffScope,
    files_mode: FilesMode,
    view_mode: ViewMode,
    split_when_auto: bool,
    selected_file: usize,
    selected_row: usize,
    selected_side: DiffSide,
    draft: Option<CommentDraft>,
    annotations_revision: u64,
    projection: ContentProjection,
    projection_revision: u64,
    hidden_source_cursor: Option<SourceLocation>,
    source: Option<Box<SourceView>>,
    pending_reference: Option<PendingReference>,
    navigation: Navigation,
}

impl ReviewSession {
    #[must_use]
    pub fn new(document: Arc<DiffDocument>) -> Self {
        let mut session = Self {
            presentation: DiffPresentation::new(document.clone(), PresentationOptions::default()),
            scope: document.target.scope().unwrap_or_default(),
            document,
            review: Review::default(),
            files_mode: FilesMode::Diff,
            view_mode: ViewMode::Auto,
            split_when_auto: false,
            selected_file: 0,
            selected_row: 0,
            selected_side: DiffSide::New,
            draft: None,
            annotations_revision: 0,
            projection: ContentProjection::default(),
            projection_revision: 0,
            hidden_source_cursor: None,
            source: None,
            pending_reference: None,
            navigation: Navigation::default(),
        };
        session.review.reconcile(&session.document);
        session.selected_file = session.presentation.file_order().first().copied().unwrap_or(0);
        session.select_first_row();
        session
    }

    #[must_use]
    pub const fn document(&self) -> &Arc<DiffDocument> {
        &self.document
    }

    #[must_use]
    pub const fn presentation(&self) -> &DiffPresentation {
        &self.presentation
    }

    #[must_use]
    pub fn target(&self) -> &ReviewTarget {
        &self.document.target
    }

    #[must_use]
    pub const fn scope(&self) -> DiffScope {
        self.scope
    }

    #[must_use]
    pub fn commits(&self) -> Option<&Arc<CommitStack>> {
        self.document.commits.as_ref()
    }

    #[must_use]
    pub fn page_label(&self) -> Option<PageLabel> {
        self.commits().map(|commits| self.target().label(Some(commits)))
    }

    #[must_use]
    pub fn commit_pages(&self, since_epoch: Duration) -> Option<Vec<CommitPage>> {
        let commits = self.commits()?;
        let current = commits.page_index(self.target());
        let now = since_epoch.as_secs();
        let working_tree = ReviewTarget::WorkingTree(self.scope);
        let pages = commits
            .commits
            .iter()
            .map(|commit| (ReviewTarget::Commit(commit.id.clone()), Some(commit)))
            .chain([(working_tree, None)])
            .enumerate()
            .map(|(index, (target, commit))| {
                let is_current = current == Some(index);
                let stat =
                    commit.and_then(|commit| commit.stat).or_else(|| is_current.then(|| self.document.line_stat()));
                let subject = commit.map_or(UNCOMMITTED_CHANGES, |commit| commit.subject.as_str());
                CommitPage {
                    number: index + 1,
                    short_id: commit.map(|commit| commit.id.short().to_owned()),
                    subject: subject.to_owned(),
                    title: CommitTitle::parse(subject),
                    body: commit.map_or_default(|commit| commit_description(&commit.body)),
                    stat,
                    age: commit
                        .and_then(|commit| commit.committed_at)
                        .and_then(|time| u64::try_from(time).ok())
                        .map(|time| format_age(now.saturating_sub(time))),
                    comments: self.review.count_for(&target),
                    current: is_current,
                    target,
                }
            })
            .collect();
        Some(pages)
    }

    #[must_use]
    pub fn page_target(&self, index: usize) -> Option<ReviewTarget> {
        let commits = self.commits()?;
        let index = index.min(commits.page_count() - 1);
        (commits.page_index(self.target()) != Some(index)).then(|| commits.page(index, self.scope)).flatten()
    }

    #[must_use]
    pub fn stepped_target(&self, delta: isize) -> Option<ReviewTarget> {
        let commits = self.commits()?;
        let current = commits.page_index(self.target())?;
        let next = current.checked_add_signed(delta)?;
        (next != current).then(|| commits.page(next, self.scope)).flatten()
    }

    #[must_use]
    pub fn command_available(&self, command: &DiffReviewCommand) -> bool {
        if self.shows_source()
            && matches!(
                command,
                DiffReviewCommand::RevealGap(_)
                    | DiffReviewCommand::ToggleFullFile
                    | DiffReviewCommand::ToggleFileCollapsed(_)
                    | DiffReviewCommand::ToggleAllFilesCollapsed
                    | DiffReviewCommand::SetViewMode(_)
                    | DiffReviewCommand::ToggleLayout
                    | DiffReviewCommand::SelectSide(_)
                    | DiffReviewCommand::Review(
                        ReviewCommand::BeginComment
                            | ReviewCommand::EditComment
                            | ReviewCommand::DeleteComment
                            | ReviewCommand::UndoComment
                    )
            )
        {
            return false;
        }
        match command {
            DiffReviewCommand::OpenCommitPicker => self.commits().is_some(),
            DiffReviewCommand::StepTarget(delta) => self.stepped_target(*delta).is_some(),
            DiffReviewCommand::GoToPage(index) => self.page_target(*index).is_some(),
            DiffReviewCommand::NavigateBack => self.back_location().is_some(),
            DiffReviewCommand::NavigateForward => self.forward_location().is_some(),
            DiffReviewCommand::CopyReview => !self.review().is_empty(),
            DiffReviewCommand::ToggleMarkdownPreview(index) => {
                self.can_preview_markdown(index.unwrap_or(self.selected_file))
            }
            _ => true,
        }
    }

    #[must_use]
    pub fn target_event(&self, command: &DiffReviewCommand) -> Option<DiffReviewEvent> {
        let target = match command {
            DiffReviewCommand::SetScope(scope) => ReviewTarget::WorkingTree(*scope),
            DiffReviewCommand::CycleScope => ReviewTarget::WorkingTree(self.scope.next()),
            DiffReviewCommand::StepTarget(delta) => self.stepped_target(*delta)?,
            DiffReviewCommand::GoToPage(index) => self.page_target(*index)?,
            _ => return None,
        };
        Some(DiffReviewEvent::SetTarget(target))
    }

    const fn presentation_options(&self) -> PresentationOptions {
        PresentationOptions {
            view_mode: self.view_mode,
            split_when_auto: self.split_when_auto,
            include_file_headers: true,
            source: match &self.source {
                Some(s) => Some(SourceFocus { file_index: self.selected_file, preview: s.preview }),
                None => None,
            },
        }
    }

    #[must_use]
    pub fn selected_file(&self) -> Option<usize> {
        (!self.displayed_document().files.is_empty()).then_some(self.selected_file)
    }

    #[must_use]
    pub fn selected_row(&self) -> Option<usize> {
        (self.presentation.row_count() != 0).then_some(self.selected_row)
    }

    #[must_use]
    pub const fn selected_side(&self) -> DiffSide {
        self.selected_side
    }

    #[must_use]
    pub fn selected_file_range(&self) -> Option<Range<usize>> {
        self.presentation.file_range(self.selected_file)
    }

    #[must_use]
    pub fn visible_range(&self) -> Range<usize> {
        0..self.presentation.row_count()
    }

    #[must_use]
    pub const fn projection_revision(&self) -> u64 {
        self.projection_revision
    }

    #[must_use]
    pub fn selected_source_line(&self) -> Option<SourceLocation> {
        let row = self.selected_presented_row()?;
        self.presentation.source_location(row, row.preferred_cell(self.selected_side)?)
    }

    fn file(&self, index: usize) -> Option<&FileDiff> {
        self.displayed_document().files.get(index)
    }

    fn displayed_document(&self) -> &Arc<DiffDocument> {
        self.source.as_ref().and_then(|s| s.reference.as_ref()).unwrap_or(&self.document)
    }

    fn focus_source(&mut self, target: TargetLocation, reference: Option<Arc<DiffDocument>>, preview: bool) {
        let return_to = match self.source.take() {
            Some(view) => view.return_to,
            None => self.capture_bookmark(),
        };
        self.source = Some(Box::new(SourceView { target, reference, preview, return_to }));
        self.rebuild();
    }

    fn capture_bookmark(&self) -> Option<SelectionBookmark> {
        Some(SelectionBookmark {
            path: self.file(self.selected_file)?.path.clone(),
            row: self.selected_presented_row().map(|row| row.id),
            source: self.hidden_source_cursor.clone().or_else(|| self.selected_source_line()),
            anchor: self.selected_anchor(),
            side: self.selected_side,
        })
    }

    /// Replaces the complete immutable document while preserving review and view
    /// state, including gap expansions for files whose content did not change.
    pub fn set_document(&mut self, document: Arc<DiffDocument>) {
        let target_changed = document.target != self.document.target;
        if target_changed {
            self.set_draft(None);
        }
        if let Some(scope) = document.target.scope() {
            self.scope = scope;
        }
        if self.review.reconcile(&document) {
            self.annotations_changed();
        }
        if Arc::ptr_eq(&document, &self.document) || document == self.document {
            return;
        }
        let bookmark = self.capture_bookmark();
        if target_changed {
            self.pending_reference = None;
            self.source = None;
        } else if let Some(view) = self.source.as_mut() {
            if matches!(&view.target.path, SourcePath::Repo(path) if document.file_index(path).is_some()) {
                view.reference = None;
            } else if let Some(file) = view.reference.as_ref().and_then(|d| d.files.last()) {
                view.reference = Some(document_with_reference(&document, file.clone()));
            } else {
                if self.files_mode == FilesMode::Browse && self.pending_reference.is_none() {
                    let target = TargetLocation { side: DiffSide::New, ..view.target.clone() };
                    self.pending_reference = Some(PendingReference { target, error: None });
                }
                self.source = None;
            }
        }
        self.projection.retain_for_document(&self.document, &document);
        self.navigation.document_changed(&document, target_changed);
        self.document = document;
        self.rebuild_at(bookmark, false);
    }

    pub fn select_file(&mut self, index: usize) -> bool {
        if index >= self.document.files.len() {
            return false;
        }
        if self.source.is_some() {
            if self.selected_file == index {
                return true;
            }
            self.leave_source_view();
        }
        self.hidden_source_cursor = None;
        self.cancel_pending_reference();
        self.selected_file = index;
        self.set_draft(None);
        self.select_first_row();
        true
    }

    pub fn move_file(&mut self, delta: isize) -> Option<usize> {
        let index = self.presentation.offset_file(self.selected_file, delta)?;
        self.select_file(index).then_some(index)
    }

    pub fn move_row(&mut self, delta: isize) -> bool {
        let range = self.visible_range();
        let navigable = |row: &usize| self.presentation.is_navigable(*row);
        let steps = delta.unsigned_abs();
        let target = if delta.is_negative() {
            (range.start..self.selected_row.min(range.end)).rev().filter(navigable).take(steps).last()
        } else {
            (self.selected_row.saturating_add(1).max(range.start)..range.end).filter(navigable).take(steps).last()
        };
        target.is_some_and(|row| self.select_row(row))
    }

    pub fn select_boundary(&mut self, end: bool) -> bool {
        let range = self.visible_range();
        let target =
            if end { self.presentation.last_navigable(range) } else { self.presentation.first_navigable(range) };
        target.is_some_and(|row| self.select_row(row))
    }

    pub fn select_row(&mut self, index: usize) -> bool {
        if !self.visible_range().contains(&index) || !self.presentation.is_navigable(index) {
            return false;
        }
        self.hidden_source_cursor = None;
        self.cancel_pending_reference();
        self.selected_row = index;
        if let Some(row) = self.presentation.row(index) {
            self.selected_file = row.file_index;
        }
        self.normalize_selected_side();
        if let Some(view) = &mut self.source
            && let Some(line) = self.presentation.row(index).and_then(|row| row.source_lines().next())
        {
            view.target.line_number = line.line_number;
        }
        true
    }

    #[must_use]
    pub fn hunk_target(&self, delta: isize) -> Option<usize> {
        let file = self.document.files.get(self.selected_file)?;
        if file.hunks.is_empty() {
            return None;
        }
        let current = self.presentation.row(self.selected_row).and_then(|row| row.hunk_index).unwrap_or(0);
        let target = offset(current, delta, file.hunks.len() - 1);
        let range = self.presentation.hunk_range(self.selected_file, target)?;
        Some(self.presentation.first_navigable(range).unwrap_or(self.selected_row))
    }

    pub fn move_hunk(&mut self, delta: isize) -> bool {
        self.hidden_source_cursor = None;
        let Some(target) = self.hunk_target(delta) else {
            return false;
        };
        self.selected_row = target;
        self.normalize_selected_side();
        true
    }

    pub fn set_selected_side(&mut self, side: DiffSide) -> bool {
        self.hidden_source_cursor = None;
        let available = self
            .selected_row()
            .and_then(|index| self.presentation.row(index))
            .is_some_and(|row| row.cell(side).is_some());
        if available {
            self.selected_side = side;
        }
        available
    }

    #[must_use]
    pub fn selected_presented_row(&self) -> Option<&PresentedRow> {
        self.presentation.row(self.selected_row)
    }

    #[must_use]
    pub fn selected_layout(&self) -> Layout {
        self.selected_presented_row().map_or(self.layout(), |row| self.presentation.file_layout(row.file_index))
    }

    #[must_use]
    pub fn selected_cell(&self) -> Option<&PresentedCell> {
        self.selected_presented_row()?.preferred_cell(self.selected_side)
    }

    #[must_use]
    pub fn selected_anchor(&self) -> Option<LineAnchor> {
        let row = self.selected_presented_row()?;
        self.presentation.cell_anchor(row, row.preferred_cell(self.selected_side)?)
    }

    fn rebuild(&mut self) {
        let bookmark = if self.source.is_none() { self.capture_bookmark() } else { None };
        self.rebuild_at(bookmark, false);
    }

    fn rebuild_at(&mut self, bookmark: Option<SelectionBookmark>, returning: bool) {
        self.selected_file = match &self.source {
            Some(view) if view.reference.is_some() => self.document.files.len(),
            Some(view) => self.document.file_index(&view.target.path.display_path()).expect("focused file in document"),
            None => bookmark.as_ref().and_then(|b| self.document.file_index(&b.path)).unwrap_or(0),
        };
        self.presentation = DiffPresentation::with_projection(
            Arc::clone(self.displayed_document()),
            self.presentation_options(),
            &self.projection,
        );
        self.projection_revision = self.projection_revision.wrapping_add(1);
        self.hidden_source_cursor = None;
        if let Some(view) = &mut self.source {
            let file = &self.presentation.document().files[self.selected_file];
            view.target.side = file.source_side();
            self.selected_side = view.target.side;
            if let Some(source) = file.source_document(self.selected_side) {
                view.target.line_number = view.target.line_number.clamp(1, source.line_count().max(1));
            }
            self.selected_row = self
                .presentation
                .source_row(self.selected_file, self.selected_side, view.target.line_number)
                .unwrap_or(0);
        } else {
            let restored = bookmark.and_then(|b| {
                if self.file(self.selected_file)?.path != b.path {
                    return None;
                }
                self.selected_side = b.side;
                let source_row = b.source.as_ref().and_then(|s| self.presentation.row_showing_source(s));
                if source_row.is_none() {
                    self.hidden_source_cursor = b.source;
                }
                let patch_row = || {
                    b.row
                        .and_then(|id| self.presentation.row_with_id(id))
                        .or_else(|| b.anchor.as_ref().and_then(|a| self.presentation.row_showing_anchor(a)))
                };
                if returning { patch_row().or(source_row) } else { source_row.or_else(patch_row) }
            });
            if let Some(row) = restored {
                self.selected_row = row;
                self.normalize_selected_side();
            } else {
                self.select_first_row();
            }
        }
        if let Some(draft) = &self.draft {
            let current = self
                .presentation
                .row_showing_anchor(&draft.anchor)
                .and_then(|row| self.presentation.anchor_at(row, draft.anchor.side));
            if current.as_ref() != Some(&draft.anchor) {
                self.set_draft(None);
            }
        }
    }

    fn select_first_row(&mut self) {
        match self.selected_file_range() {
            Some(range) => {
                self.selected_row = self
                    .presentation
                    .first_source_row(range.clone())
                    .or_else(|| self.presentation.first_navigable(range.clone()))
                    .unwrap_or(range.start);
            }
            None => self.selected_row = 0,
        }
        self.normalize_selected_side();
    }

    fn normalize_selected_side(&mut self) {
        let Some(row) = self.presentation.row(self.selected_row) else {
            return;
        };
        if row.cell(self.selected_side).is_none() && row.cell(self.selected_side.opposite()).is_some() {
            self.selected_side = self.selected_side.opposite();
        }
    }
}

fn offset(value: usize, delta: isize, maximum: usize) -> usize {
    if delta.is_negative() {
        value.saturating_sub(delta.unsigned_abs())
    } else {
        value.saturating_add(delta.unsigned_abs()).min(maximum)
    }
}
