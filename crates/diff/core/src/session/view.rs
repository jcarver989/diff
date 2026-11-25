use super::ReviewSession;
use crate::{DiffFileView, DiffSide, FilePresentation, GapId, Layout, RepoPath, RowKind, SourceLocation, ViewMode};
use serde::{Deserialize, Serialize};

const EXPAND_STEP: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevealAmount {
    Step,
    All,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilesMode {
    Browse,
    #[default]
    Diff,
}

impl FilesMode {
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Browse => "Browse",
            Self::Diff => "Diff",
        }
    }

    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Browse => Self::Diff,
            Self::Diff => Self::Browse,
        }
    }
}

impl ReviewSession {
    #[must_use]
    pub const fn view_mode(&self) -> ViewMode {
        self.view_mode
    }

    #[must_use]
    pub const fn files_mode(&self) -> FilesMode {
        self.files_mode
    }

    pub fn set_files_mode(&mut self, mode: FilesMode) -> bool {
        if self.files_mode == mode || self.draft.is_some() {
            return false;
        }
        self.files_mode = mode;
        match mode {
            FilesMode::Browse if !self.shows_source() => {
                self.enter_source_view();
            }
            FilesMode::Browse => {}
            FilesMode::Diff => {
                self.cancel_pending_reference();
                if self.source_view().is_some() {
                    self.leave_source_view();
                }
            }
        }
        true
    }

    #[must_use]
    pub const fn layout(&self) -> Layout {
        self.presentation_options().layout()
    }

    #[must_use]
    pub fn source_view(&self) -> Option<&RepoPath> {
        self.source.as_ref()?;
        Some(&self.file(self.selected_file)?.path)
    }

    #[must_use]
    pub fn source_view_label(&self) -> Option<&'static str> {
        let browsing = self.files_mode == FilesMode::Browse;
        if self.reference().is_some() {
            return Some(if browsing { "Unchanged" } else { "Not in this diff · read-only · o back to diff" });
        }
        Some(match (self.source.as_ref()?.target.side, browsing) {
            (DiffSide::New, true) => "Changed · o shows the diff",
            (DiffSide::Old, true) => "Deleted · o shows the diff",
            (DiffSide::New, false) => "Source (new) · o back to diff",
            (DiffSide::Old, false) => "Source (old — deleted) · o back to diff",
        })
    }

    pub fn toggle_source_view(&mut self) -> bool {
        if self.draft.is_some() {
            return false;
        }
        if self.source_view().is_some() {
            self.leave_source_view();
            return true;
        }
        self.enter_source_view()
    }

    pub(super) fn leave_source_view(&mut self) {
        let bookmark = self.source.take().and_then(|s| s.return_to);
        self.rebuild_at(bookmark, true);
    }

    fn enter_source_view(&mut self) -> bool {
        let Some(file) = self.document.files.get(self.selected_file) else {
            return false;
        };
        let (path, side) = (file.path.clone(), file.source_side());
        let range = self.selected_file_range().unwrap_or(0..0);
        let line_number = self
            .presentation
            .rows(self.selected_row..range.end)
            .iter()
            .chain(self.presentation.rows(range.start..self.selected_row).iter().rev())
            .find_map(|row| row.cell(side)?.line_number())
            .unwrap_or(1);
        self.focus_source(SourceLocation { path, side, line_number }.into(), None, false);
        true
    }

    pub fn expand_selected(&mut self, amount: RevealAmount) -> bool {
        if self.shows_source() {
            return false;
        }
        if self.selected_on_header(self.selected_file) {
            return self.set_patch_view(self.selected_file, DiffFileView::Patch);
        }
        let id = self.presentation.gap_info(self.selected_row).map(|info| info.id).or_else(|| {
            let row = self.presentation.row(self.selected_row)?;
            let source = row.preferred_cell(self.selected_side)?.patch_source?;
            let hunk = self.file(row.file_index)?.hunks.get(source.hunk_index)?;
            let after = source.line_index.saturating_mul(2) >= hunk.lines.len();
            Some(GapId { file_index: row.file_index, gap_index: source.hunk_index + usize::from(after) })
        });
        let Some(id) = id else {
            return false;
        };
        let Some(file) = self.file(id.file_index) else {
            return false;
        };
        let hunk_count = file.hunks.len();
        let amount = match amount {
            RevealAmount::Step => EXPAND_STEP,
            RevealAmount::All => usize::MAX,
        };
        let prefix = if id.gap_index != 0 || hunk_count == 0 { amount } else { 0 };
        let suffix = if id.gap_index != hunk_count || hunk_count == 0 { amount } else { 0 };
        self.projection.expand_gap(id, prefix, suffix);
        self.rebuild();
        true
    }

    pub fn toggle_full_file(&mut self) -> bool {
        if self.shows_source() {
            return false;
        }
        let Some(file) = self.document.files.get(self.selected_file) else {
            return false;
        };
        if file.binary || file.omitted_bytes.is_some() {
            return false;
        }
        self.projection.toggle_full_file(file.path.clone());
        self.rebuild();
        true
    }

    #[must_use]
    pub fn is_file_collapsed(&self, index: usize) -> bool {
        self.presentation.is_collapsed(index)
    }

    #[must_use]
    pub fn all_files_collapsed(&self) -> bool {
        !self.document.files.is_empty()
            && (0..self.document.files.len()).all(|index| self.presentation.is_collapsed(index))
    }

    pub(super) fn can_preview_markdown(&self, index: usize) -> bool {
        (!self.shows_source() || (self.source.is_some() && index == self.selected_file))
            && self.file(index).is_some_and(|file| file.markdown_source().is_some())
    }

    pub fn toggle_file_collapsed(&mut self, index: Option<usize>) -> bool {
        let index = index.unwrap_or(self.selected_file);
        let next = if self.is_file_collapsed(index) { DiffFileView::Patch } else { DiffFileView::Collapsed };
        !self.shows_source() && self.set_patch_view(index, next)
    }

    pub fn toggle_markdown_preview(&mut self, index: Option<usize>) -> bool {
        let index = index.unwrap_or(self.selected_file);
        if !self.can_preview_markdown(index) {
            return false;
        }
        let preview = self.presentation.file_presentation(index) != FilePresentation::Markdown;
        if let Some(view) = &mut self.source {
            view.preview = preview;
            self.rebuild();
            true
        } else {
            self.set_patch_view(index, if preview { DiffFileView::Preview } else { DiffFileView::Patch })
        }
    }

    fn set_patch_view(&mut self, index: usize, next: DiffFileView) -> bool {
        let Some(file) = self.document.files.get(index) else {
            return false;
        };
        if self.projection.file_view(&file.path) == next
            || (next != DiffFileView::Patch && self.draft.as_ref().is_some_and(|d| d.anchor.path == file.path))
        {
            return false;
        }
        self.projection.set_file_view(file.path.clone(), next);
        self.rebuild();
        if next == DiffFileView::Patch && self.selected_on_header(index) {
            self.select_first_row();
        }
        true
    }

    pub fn toggle_all_files_collapsed(&mut self) -> bool {
        if self.document.files.is_empty() || self.shows_source() {
            return false;
        }
        let collapse = !self.all_files_collapsed();
        if collapse && self.draft.is_some() {
            return false;
        }
        let header_selected = self.selected_on_header(self.selected_file);
        let view = if collapse { DiffFileView::Collapsed } else { DiffFileView::Patch };
        for file in &self.document.files {
            self.projection.set_file_view(file.path.clone(), view);
        }
        self.rebuild();
        if header_selected && !collapse {
            self.select_first_row();
        }
        true
    }

    fn selected_on_header(&self, index: usize) -> bool {
        self.selected_presented_row().is_some_and(|row| row.kind == RowKind::FileHeader && row.file_index == index)
    }

    pub fn set_view_mode(&mut self, mode: ViewMode) -> bool {
        if self.view_mode == mode {
            return false;
        }
        let previous = self.layout();
        self.view_mode = mode;
        if self.layout() != previous {
            self.rebuild();
        }
        true
    }

    pub fn toggle_layout(&mut self) -> bool {
        self.set_view_mode(self.view_mode.resolve(self.split_when_auto).toggled().into())
    }

    #[must_use]
    pub const fn split_when_auto(&self) -> bool {
        self.split_when_auto
    }

    pub fn set_split_when_auto(&mut self, split: bool) -> bool {
        if self.split_when_auto == split {
            return false;
        }
        let previous = self.layout();
        self.split_when_auto = split;
        if self.layout() == previous {
            return false;
        }
        self.rebuild();
        true
    }
}
