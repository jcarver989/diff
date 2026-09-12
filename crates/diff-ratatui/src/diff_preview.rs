//! Compact, bounded rendering for replaceable in-progress diff previews.

use crate::{
    color::{layered_style, page_color},
    syntax::highlighted_line,
    text::{FitOptions, FitPosition, fit_spans_from},
};
use clankerdiff_core::{
    DiffDocument, DiffPresentation, FileDiff, Layout, PresentationOptions, PresentedCell,
    PresentedRow, RowKind, ViewMode,
};
use clankerdiff_syntax::{
    HighlightSpan, LanguageHint, SyntaxHighlighter, SyntaxTheme, empty_spans,
};
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
    pub tab_width: u16,
}

impl Default for DiffPreviewOptions {
    fn default() -> Self {
        Self {
            max_content_rows: 20,
            view_mode: ViewMode::Auto,
            include_hunk_headers: true,
            overflow_summary: true,
            tab_width: 2,
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

pub(crate) fn cell_highlights(
    highlighter: &mut SyntaxHighlighter,
    theme: &SyntaxTheme,
    presentation: &DiffPresentation,
    row: &PresentedRow,
    cell: &PresentedCell,
) -> Arc<[HighlightSpan]> {
    let source = presentation.cell_context(row, cell);
    highlighter
        .with_theme(theme)
        .highlight_document(source.id, LanguageHint::Path(source.path), || source.text())
        .ok()
        .and_then(|highlights| highlights.line_shared(source.target_line))
        .unwrap_or_else(empty_spans)
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
    let mut renderer = PreviewRenderer {
        presentation,
        theme,
        highlighter,
        width,
        tab_width: options.tab_width,
    };
    let mut lines = Vec::new();
    let mut overflow = 0;
    for (index, row) in eligible.iter().enumerate() {
        let remaining = options.max_content_rows.saturating_sub(lines.len());
        let segments = if remaining == 0 {
            Vec::new()
        } else {
            renderer.render(row, remaining.saturating_add(1))
        };
        let truncated = remaining == 0 || segments.len() > remaining;
        lines.extend(segments.into_iter().take(remaining));
        if truncated {
            overflow = eligible.len() - index;
            break;
        }
    }
    if options.overflow_summary && overflow > 0 {
        lines.push(fit_line(
            Line::styled(
                format!("… {overflow} more rows"),
                page_style(theme).fg(page_color(theme, theme.diff.muted)),
            ),
            usize::from(width),
            options.tab_width,
        ));
    }
    lines
}

struct PreviewRenderer<'a> {
    presentation: &'a DiffPresentation,
    theme: &'a ReviewTheme,
    highlighter: &'a mut SyntaxHighlighter,
    width: u16,
    tab_width: u16,
}

impl PreviewRenderer<'_> {
    fn render(&mut self, row: &PresentedRow, limit: usize) -> Vec<Line<'static>> {
        match self.presentation.layout() {
            Layout::Unified => match row.primary_cell() {
                Some(cell) => self.render_cell(row, cell, self.width, limit),
                None => vec![self.blank(None, self.width)],
            },
            Layout::Split => {
                let half = self.width.saturating_sub(1) / 2;
                let right_width = self.width.saturating_sub(1).saturating_sub(half);
                let mut left = row
                    .left
                    .as_ref()
                    .map_or_else(Vec::new, |cell| self.render_cell(row, cell, half, limit));
                let mut right = row.right.as_ref().map_or_else(Vec::new, |cell| {
                    self.render_cell(row, cell, right_width, limit)
                });
                let height = left.len().max(right.len()).max(1);
                left.resize_with(height, || self.blank(row.left.as_ref(), half));
                right.resize_with(height, || self.blank(row.right.as_ref(), right_width));
                let divider = Span::styled(
                    "│",
                    page_style(self.theme).fg(page_color(self.theme, self.theme.diff.border)),
                );
                left.into_iter()
                    .zip(right)
                    .map(|(left, right)| {
                        let mut spans = left.spans;
                        spans.push(divider.clone());
                        spans.extend(right.spans);
                        Line::from(spans)
                    })
                    .collect()
            }
        }
    }

    fn cell_style(&self, cell: &PresentedCell) -> Style {
        let colors = self.theme.diff.tone(cell.tone);
        layered_style(
            colors.foreground,
            colors.background,
            self.theme.diff.background,
        )
    }

    fn blank(&self, cell: Option<&PresentedCell>, width: u16) -> Line<'static> {
        let style = cell.map_or_else(|| page_style(self.theme), |cell| self.cell_style(cell));
        pad_line(Line::default().style(style), usize::from(width))
    }

    fn render_cell(
        &mut self,
        row: &PresentedRow,
        cell: &PresentedCell,
        width: u16,
        limit: usize,
    ) -> Vec<Line<'static>> {
        let base = self.cell_style(cell);
        let marker = cell.tone.marker();
        let number = cell
            .line_number()
            .map_or_else(String::new, |line| line.to_string());
        let number_width = number.len().max(4);
        let gutter = |number: &str| format!("{number:>number_width$} {marker} ");
        let width = usize::from(width);
        let gutter_width = number_width + 3;
        if width <= gutter_width {
            return vec![fit_line(
                Line::styled(gutter(&number), base),
                width,
                self.tab_width,
            )];
        }
        let spans = cell_highlights(
            self.highlighter,
            &self.theme.syntax,
            self.presentation,
            row,
            cell,
        );
        let content = highlighted_line(&cell.text, &spans, base).spans;
        let content_width = width - gutter_width;
        let wrap = matches!(row.kind, RowKind::Code | RowKind::ExpandedContext);
        fit(content, content_width, wrap, self.tab_width)
            .take(limit)
            .enumerate()
            .map(|(segment, line)| {
                let mut line = pad_line(line.style(base), content_width);
                line.spans.insert(
                    0,
                    Span::styled(gutter(if segment == 0 { &number } else { "↪" }), base),
                );
                line
            })
            .collect()
    }
}

/// Clips a line to `width` cells and pads it with the line's base style.
fn fit_line(line: Line<'static>, width: usize, tab_width: u16) -> Line<'static> {
    let base = line.style;
    let fitted = fit(line.spans, width, false, tab_width)
        .next()
        .unwrap_or_default();
    pad_line(fitted.style(base), width)
}

fn page_style(theme: &ReviewTheme) -> Style {
    Style::new().bg(page_color(theme, theme.diff.background))
}

fn fit(
    spans: Vec<Span<'_>>,
    width: usize,
    wrap: bool,
    tab_width: u16,
) -> impl Iterator<Item = Line<'static>> {
    fit_spans_from(
        spans,
        FitOptions {
            width,
            wrap,
            tab_width: usize::from(tab_width),
            continuation: "",
        },
        FitPosition::default(),
    )
    .map(|(line, _)| line)
}

fn pad_line(mut line: Line<'static>, width: usize) -> Line<'static> {
    let used = line.width();
    line.spans.push(Span::styled(
        " ".repeat(width.saturating_sub(used)),
        line.style,
    ));
    line
}
