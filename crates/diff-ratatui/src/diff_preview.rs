//! Compact, bounded rendering for replaceable in-progress diff previews.

use crate::{
    color::{layered_style, page_color},
    syntax::highlighted_line,
    text::{FitOptions, fit_spans},
};
use clankerdiff_core::{
    DiffDocument, DiffPresentation, FileDiff, Layout, PresentationOptions, PresentedCell,
    PresentedRow, RowKind, ViewMode,
};
use clankerdiff_syntax::{HighlightSpan, LanguageHint, SyntaxHighlighter, SyntaxTheme};
use clankerdiff_theme::{Fingerprint, ReviewTheme};
use ratatui::{
    style::Style,
    text::{Line, Span},
};
use std::sync::Arc;

const SPLIT_BREAKPOINT: u16 = 96;

/// Controls compact preview layout and truncation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffPreviewOptions {
    pub max_content_rows: usize,
    pub view_mode: ViewMode,
    pub include_hunk_headers: bool,
    pub overflow_summary: bool,
}

impl Default for DiffPreviewOptions {
    fn default() -> Self {
        Self {
            max_content_rows: 20,
            view_mode: ViewMode::Auto,
            include_hunk_headers: true,
            overflow_summary: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiffPreviewStats {
    pub presentations_built: usize,
    pub rows_generated: usize,
    pub cache_hits: usize,
}

type PresentationKey = (ViewMode, bool);
type RenderKey = (u16, DiffPreviewOptions, Fingerprint);

#[derive(Debug)]
pub struct DiffPreviewState {
    document: Arc<DiffDocument>,
    presentation: Option<(PresentationKey, DiffPresentation)>,
    rendered: Option<(RenderKey, Arc<[Line<'static>]>)>,
    stats: DiffPreviewStats,
}

impl DiffPreviewState {
    #[must_use]
    pub fn new(file: FileDiff) -> Self {
        Self {
            document: preview_document(file),
            presentation: None,
            rendered: None,
            stats: DiffPreviewStats::default(),
        }
    }

    pub fn set_file(&mut self, file: FileDiff) {
        *self = Self {
            stats: self.stats,
            ..Self::new(file)
        };
    }

    pub fn take_stats(&mut self) -> DiffPreviewStats {
        std::mem::take(&mut self.stats)
    }

    pub fn render(
        &mut self,
        width: u16,
        theme: &ReviewTheme,
        highlighter: &mut SyntaxHighlighter,
        options: DiffPreviewOptions,
    ) -> Arc<[Line<'static>]> {
        let key = (width, options, theme.revision());
        if let Some((cached, rows)) = &self.rendered
            && *cached == key
        {
            self.stats.cache_hits += 1;
            return Arc::clone(rows);
        }
        let presentation_key = (options.view_mode, width >= SPLIT_BREAKPOINT);
        let presentation = match &mut self.presentation {
            Some((cached, presentation)) if *cached == presentation_key => presentation,
            slot => {
                self.stats.presentations_built += 1;
                let presentation = preview_presentation(Arc::clone(&self.document), width, options);
                &slot.insert((presentation_key, presentation)).1
            }
        };
        let rows: Arc<[Line<'static>]> =
            render_preview_rows(presentation, width, theme, highlighter, options).into();
        self.stats.rows_generated += rows.len();
        self.rendered = Some((key, Arc::clone(&rows)));
        rows
    }
}

fn preview_document(file: FileDiff) -> Arc<DiffDocument> {
    Arc::new(DiffDocument {
        repo_root: String::new(),
        files: vec![file],
    })
}

fn preview_presentation(
    document: Arc<DiffDocument>,
    width: u16,
    options: DiffPreviewOptions,
) -> DiffPresentation {
    DiffPresentation::new(
        document,
        PresentationOptions {
            view_mode: options.view_mode,
            split_when_auto: width >= SPLIT_BREAKPOINT,
            include_file_headers: false,
        },
    )
}

/// Highlights one presented cell, preferring its complete source version, then
/// its bounded hunk-side sequence, and finally a path-hinted single-line
/// highlight for synthetic cells and oversized hunks.
pub(crate) fn cell_highlights(
    highlighter: &mut SyntaxHighlighter,
    theme: &SyntaxTheme,
    presentation: &DiffPresentation,
    row: &PresentedRow,
    cell: &PresentedCell,
) -> Arc<[HighlightSpan]> {
    let mut syntax = highlighter.with_theme(theme);
    if let (Some(source), Some(path), Some(line)) = (
        presentation.source_document(row, cell),
        presentation.source_path(row, cell),
        cell.line_number().and_then(|line| line.checked_sub(1)),
    ) {
        return syntax
            .highlight_document(
                source.sequence_id(),
                LanguageHint::Path(path),
                source.text(),
            )
            .line_shared(line)
            .unwrap_or_else(clankerdiff_syntax::empty_spans);
    }
    if let Some(sequence) = presentation.hunk_sequence(row, cell) {
        return syntax
            .highlight_document_lines(
                sequence.id,
                LanguageHint::Path(sequence.path),
                sequence.lines(),
            )
            .line_shared(sequence.target_line)
            .unwrap_or_else(clankerdiff_syntax::empty_spans);
    }
    syntax.highlight_source(LanguageHint::Path(presentation.row_path(row)), &cell.text)
}

/// Renders one file without constructing review state or executing Git.
///
/// Hosts can call this for each replacement snapshot produced by
/// `FileDiff::from_texts`; no watcher or incremental patch transport is needed.
#[must_use]
pub fn render_diff_preview(
    file: FileDiff,
    width: u16,
    theme: &ReviewTheme,
    highlighter: &mut SyntaxHighlighter,
    options: DiffPreviewOptions,
) -> Vec<Line<'static>> {
    let presentation = preview_presentation(preview_document(file), width, options);
    render_preview_rows(&presentation, width, theme, highlighter, options)
}

fn render_preview_rows(
    presentation: &DiffPresentation,
    width: u16,
    theme: &ReviewTheme,
    highlighter: &mut SyntaxHighlighter,
    options: DiffPreviewOptions,
) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let eligible = presentation
        .rows(0..presentation.row_count())
        .iter()
        .filter(|row| options.include_hunk_headers || row.kind != RowKind::HunkHeader)
        .collect::<Vec<_>>();
    let shown = eligible.len().min(options.max_content_rows);
    let mut lines = eligible
        .iter()
        .take(shown)
        .map(|row| {
            let mut cell_line =
                |cell, width| render_cell(presentation, row, cell, width, theme, highlighter);
            match presentation.layout() {
                Layout::Unified => row
                    .primary_cell()
                    .map_or_else(Line::default, |cell| cell_line(cell, width)),
                Layout::Split => {
                    let half = width.saturating_sub(1) / 2;
                    let right_width = width.saturating_sub(1).saturating_sub(half);
                    let blank = |width| {
                        Line::styled(
                            " ".repeat(usize::from(width)),
                            Style::new().bg(page_color(theme, theme.diff.background)),
                        )
                    };
                    let left = row
                        .left
                        .as_ref()
                        .map_or_else(|| blank(half), |cell| cell_line(cell, half));
                    let right = row
                        .right
                        .as_ref()
                        .map_or_else(|| blank(right_width), |cell| cell_line(cell, right_width));
                    let mut spans = left.spans;
                    spans.push(Span::styled(
                        "│",
                        Style::new().fg(page_color(theme, theme.diff.border)),
                    ));
                    spans.extend(right.spans);
                    Line::from(spans)
                }
            }
        })
        .collect::<Vec<_>>();
    let overflow = eligible.len().saturating_sub(shown);
    if options.overflow_summary && overflow > 0 {
        lines.push(fit_line(
            Line::styled(
                format!("… {overflow} more rows"),
                Style::new()
                    .fg(page_color(theme, theme.diff.muted))
                    .bg(page_color(theme, theme.diff.background)),
            ),
            usize::from(width),
        ));
    }
    lines
}

fn render_cell(
    presentation: &DiffPresentation,
    row: &PresentedRow,
    cell: &PresentedCell,
    width: u16,
    theme: &ReviewTheme,
    highlighter: &mut SyntaxHighlighter,
) -> Line<'static> {
    let colors = theme.diff.tone(cell.tone);
    let base = layered_style(colors.foreground, colors.background, theme.diff.background);
    let marker = cell.tone.marker();
    let number = cell
        .line_number()
        .map_or_else(|| "    ".to_owned(), |line| format!("{line:>4}"));
    let prefix = format!("{number} {marker} ");
    let spans = cell_highlights(highlighter, &theme.syntax, presentation, row, cell);
    let mut line = highlighted_line(&cell.text, &spans, base);
    line.spans.insert(0, Span::styled(prefix, base));
    line.style = base;
    fit_line(line, usize::from(width))
}

/// Clips a line to `width` cells and pads it with the line's base style.
fn fit_line(line: Line<'static>, width: usize) -> Line<'static> {
    let base = line.style;
    let mut fitted = fit_spans(
        line.spans,
        FitOptions {
            width,
            wrap: false,
            tab_width: 4,
            continuation: "",
        },
    )
    .into_iter()
    .next()
    .unwrap_or_default();
    let used = fitted.width();
    fitted
        .spans
        .push(Span::styled(" ".repeat(width.saturating_sub(used)), base));
    fitted.style(base)
}
