use crate::SymbolIndex;
use clankerdiff_core::{
    DiffDocument, DiffSide, FileDiff, Hunk, PatchLine, PresentedCell, PresentedRow, RepoPath, join_lines,
};
use clankerdiff_syntax::{HighlightSpan, LanguageHint, SourceSyntax, SymbolTags, SyntaxCell, empty_spans, has_symbols};
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

pub const HUNK_WINDOW_LINES: usize = 512;

type HunkWindows = [Box<[SyntaxCell]>; 2];

#[derive(Debug, Default)]
pub struct DocumentSyntax {
    document: Arc<DiffDocument>,
    sources: Vec<[SyntaxCell; 2]>,
    hunks: Vec<Vec<HunkWindows>>,
    symbols: OnceLock<Arc<SymbolIndex>>,
}

impl DocumentSyntax {
    #[must_use]
    pub fn new(document: Arc<DiffDocument>) -> Self {
        Self::default().following(document)
    }

    #[must_use]
    pub const fn document(&self) -> &Arc<DiffDocument> {
        &self.document
    }

    pub fn sync(&mut self, document: &Arc<DiffDocument>) {
        if !Arc::ptr_eq(&self.document, document) {
            *self = self.following(Arc::clone(document));
        }
    }

    fn following(&self, document: Arc<DiffDocument>) -> Self {
        let previous: HashMap<&RepoPath, (&FileDiff, &[SyntaxCell; 2])> =
            self.document.files.iter().zip(&self.sources).map(|(file, cells)| (&file.path, (file, cells))).collect();
        let sources = document
            .files
            .iter()
            .map(|file| {
                [DiffSide::Old, DiffSide::New].map(|side| match previous.get(&file.path) {
                    Some((before, cells)) if same_source(before, file, side) => cells[side as usize].clone(),
                    _ => SyntaxCell::default(),
                })
            })
            .collect();
        let windows = |hunk: &Hunk| -> HunkWindows {
            [(); 2].map(|()| (0..hunk.lines.len().div_ceil(HUNK_WINDOW_LINES)).map(|_| SyntaxCell::default()).collect())
        };
        let hunks = document.files.iter().map(|file| file.hunks.iter().map(windows).collect()).collect();
        Self { document, sources, hunks, symbols: OnceLock::new() }
    }

    #[must_use]
    pub fn highlights(&self, row: &PresentedRow, cell: &PresentedCell) -> Arc<[HighlightSpan]> {
        self.cell_highlights(row, cell).unwrap_or_else(empty_spans)
    }

    #[must_use]
    pub fn symbols(&self) -> Arc<SymbolIndex> {
        Arc::clone(self.symbols.get_or_init(|| Arc::new(SymbolIndex::new(self))))
    }

    pub(crate) fn tags(&self, file: usize, side: DiffSide) -> Arc<SymbolTags> {
        let diff = &self.document.files[file];
        match diff.source_document(side) {
            Some(source) if has_symbols(language(diff, side), source.text()) => {
                self.source(file, side).map_or_else(Arc::default, |syntax| Arc::clone(&syntax.symbols))
            }
            _ => Arc::default(),
        }
    }

    fn source(&self, file: usize, side: DiffSide) -> Option<&SourceSyntax> {
        let diff = self.document.files.get(file)?;
        let source = diff.source_document(side)?;
        Some(self.sources.get(file)?[side as usize].get(language(diff, side), || source.text()))
    }

    fn cell_highlights(&self, row: &PresentedRow, cell: &PresentedCell) -> Option<Arc<[HighlightSpan]>> {
        if let Some(line) = cell.source_line
            && let Some(syntax) = self.source(row.file_index, line.side)
        {
            return syntax.highlights.line_shared(line.line_number.checked_sub(1)?);
        }
        let source = cell.patch_source?;
        let file = self.document.files.get(row.file_index)?;
        let hunk = file.hunks.get(source.hunk_index)?;
        let window = source.line_index / HUNK_WINDOW_LINES;
        let start = window * HUNK_WINDOW_LINES;
        let lines = hunk.lines.get(start..hunk.lines.len().min(start + HUNK_WINDOW_LINES))?;
        let on_side = |line: &&PatchLine| line.line_number(source.side).is_some();
        let line = lines.get(..source.line_index - start)?.iter().filter(on_side).count();
        let cell = self.hunks.get(row.file_index)?.get(source.hunk_index)?[source.side as usize].get(window)?;
        let syntax = cell.get(language(file, source.side), || {
            join_lines(lines.iter().filter(on_side).map(|line| line.text.as_ref()))
        });
        syntax.highlights.line_shared(line)
    }
}

fn language(file: &FileDiff, side: DiffSide) -> LanguageHint<'_> {
    LanguageHint::Path(file.path_for_side(side).as_str())
}

fn same_source(before: &FileDiff, after: &FileDiff, side: DiffSide) -> bool {
    before.path_for_side(side) == after.path_for_side(side)
        && before
            .source_document(side)
            .zip(after.source_document(side))
            .is_some_and(|(before, after)| before.content_id() == after.content_id())
}
