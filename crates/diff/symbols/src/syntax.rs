use crate::SymbolIndex;
use clankerdiff_core::{DiffDocument, DiffSide, FileDiff, Hunk, PresentedCell, PresentedRow, RepoPath, join_lines};
use clankerdiff_syntax::{HighlightLine, LanguageHint, SourceSyntax, SymbolTags, analyze, has_symbols};
use std::{
    collections::HashMap,
    mem,
    sync::{Arc, OnceLock},
};

pub const HUNK_WINDOW_LINES: usize = 512;

const OFF_SIDE: u32 = u32::MAX;

#[derive(Debug, Default)]
pub struct DocumentSyntax {
    document: Arc<DiffDocument>,
    sources: Vec<OnceLock<SourceSyntax>>,
    file_hunks: Vec<u32>,
    hunk_windows: Vec<u32>,
    windows: Vec<OnceLock<HunkWindow>>,
    prepared: Vec<OnceLock<()>>,
    symbols: OnceLock<Arc<SymbolIndex>>,
}

#[derive(Debug, Clone)]
struct HunkWindow {
    syntax: SourceSyntax,
    lines: Box<[u32]>,
}

impl DocumentSyntax {
    #[must_use]
    pub fn new(document: Arc<DiffDocument>) -> Self {
        let mut syntax = Self::default();
        syntax.follow(document);
        syntax
    }

    #[must_use]
    pub const fn document(&self) -> &Arc<DiffDocument> {
        &self.document
    }

    pub fn sync(&mut self, document: &Arc<DiffDocument>) {
        if !Arc::ptr_eq(&self.document, document) {
            self.follow(Arc::clone(document));
        }
    }

    #[must_use]
    pub fn followed(&self, document: &Arc<DiffDocument>) -> Self {
        let mut next = Self {
            document: Arc::clone(&self.document),
            sources: self
                .sources
                .iter()
                .map(|source| source.get().cloned().map(OnceLock::from).unwrap_or_default())
                .collect(),
            ..Self::default()
        };
        next.follow(Arc::clone(document));
        next
    }

    pub fn prepare(&self, file: usize) {
        let Some(diff) = self.document.files.get(file) else {
            return;
        };
        for side in [DiffSide::Old, DiffSide::New] {
            let _ = self.source(file, side);
            for (hunk_index, hunk) in diff.hunks.iter().enumerate() {
                for window in 0..window_count(hunk) as usize {
                    let _ = self.window(file, hunk_index, side, window);
                }
            }
        }
        if let Some(prepared) = self.prepared.get(file) {
            let _ = prepared.set(());
        }
    }

    #[must_use]
    pub fn is_prepared(&self, file: usize) -> bool {
        self.prepared.get(file).is_some_and(|prepared| prepared.get().is_some())
    }

    #[must_use]
    pub fn ready_highlights(&self, row: &PresentedRow, cell: &PresentedCell) -> HighlightLine<'_> {
        if self.is_prepared(row.file_index) { self.highlights(row, cell) } else { HighlightLine::default() }
    }

    #[must_use]
    pub fn ready_symbols(&self) -> Option<Arc<SymbolIndex>> {
        self.symbols.get().cloned()
    }

    fn follow(&mut self, document: Arc<DiffDocument>) {
        let previous = mem::replace(&mut self.document, document);
        let mut kept = mem::take(&mut self.sources);
        let positions: HashMap<&RepoPath, usize> =
            previous.files.iter().enumerate().map(|(index, file)| (&file.path, index)).collect();
        self.sources.reserve(self.document.files.len() * 2);
        self.file_hunks.clear();
        self.hunk_windows.clear();
        self.file_hunks.push(0);
        self.hunk_windows.push(0);
        let (mut hunks, mut windows) = (0, 0);
        for file in &self.document.files {
            let before = positions.get(&file.path).map(|&index| (index, &previous.files[index]));
            for side in [DiffSide::Old, DiffSide::New] {
                self.sources.push(match before {
                    Some((index, before)) if same_source(before, file, side) => {
                        kept.get_mut(index * 2 + side as usize).map(mem::take).unwrap_or_default()
                    }
                    _ => OnceLock::new(),
                });
            }
            for hunk in &file.hunks {
                windows += 2 * window_count(hunk);
                self.hunk_windows.push(windows);
            }
            hunks += u32::try_from(file.hunks.len()).expect("hunk count fits u32");
            self.file_hunks.push(hunks);
        }
        self.windows = (0..windows).map(|_| OnceLock::new()).collect();
        self.prepared = (0..self.document.files.len()).map(|_| OnceLock::new()).collect();
        self.symbols = OnceLock::new();
    }

    #[must_use]
    pub fn highlights(&self, row: &PresentedRow, cell: &PresentedCell) -> HighlightLine<'_> {
        self.cell_highlights(row, cell).unwrap_or_default().clipped(cell.source_text().len())
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
        let cell = self.sources.get(file * 2 + side as usize)?;
        Some(cell.get_or_init(|| analyze(language(diff, side), source.text())))
    }

    fn cell_highlights(&self, row: &PresentedRow, cell: &PresentedCell) -> Option<HighlightLine<'_>> {
        if let Some(line) = cell.source_line
            && let Some(syntax) = self.source(row.file_index, line.side)
        {
            return syntax.highlights.line(line.line_number.checked_sub(1)?);
        }
        let source = cell.patch_source?;
        let window =
            self.window(row.file_index, source.hunk_index, source.side, source.line_index / HUNK_WINDOW_LINES)?;
        let line = *window.lines.get(source.line_index % HUNK_WINDOW_LINES)?;
        if line == OFF_SIDE {
            return None;
        }
        window.syntax.highlights.line(line as usize)
    }

    fn window(&self, file: usize, hunk_index: usize, side: DiffSide, window: usize) -> Option<&HunkWindow> {
        let diff = self.document.files.get(file)?;
        let hunk = diff.hunks.get(hunk_index)?;
        let global = *self.file_hunks.get(file)? as usize + hunk_index;
        let first = *self.hunk_windows.get(global)? as usize;
        let count = (*self.hunk_windows.get(global + 1)? as usize - first) / 2;
        if window >= count {
            return None;
        }
        let slot = self.windows.get(first + side as usize * count + window)?;
        Some(slot.get_or_init(|| HunkWindow::new(diff, hunk, side, window)))
    }
}

impl HunkWindow {
    fn new(file: &FileDiff, hunk: &Hunk, side: DiffSide, window: usize) -> Self {
        let start = window * HUNK_WINDOW_LINES;
        let lines = &hunk.lines[start..hunk.lines.len().min(start + HUNK_WINDOW_LINES)];
        let mut next = 0;
        let mapping = lines
            .iter()
            .map(|line| {
                line.line_number(side).map_or(OFF_SIDE, |_| {
                    next += 1;
                    next - 1
                })
            })
            .collect();
        let text =
            join_lines(lines.iter().filter(|line| line.line_number(side).is_some()).map(|line| line.text.as_ref()));
        Self { syntax: analyze(language(file, side), &text), lines: mapping }
    }
}

fn window_count(hunk: &Hunk) -> u32 {
    u32::try_from(hunk.lines.len().div_ceil(HUNK_WINDOW_LINES)).expect("window count fits u32")
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
