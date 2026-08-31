//! Compact, bounded rendering for replaceable in-progress diff previews.

use crate::syntax::highlighted_line;
use diff_core::{
    DiffDocument, DiffPresentation, FileDiff, Layout, PresentationOptions, PresentedCell,
    PresentedRow, RowKind, ViewMode,
};
use diff_syntax::{HighlightSpan, LanguageHint, SyntaxHighlighter, SyntaxTheme};
use diff_theme::{ReviewTheme, Rgba};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use std::sync::Arc;
use unicode_width::UnicodeWidthChar;

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
            .unwrap_or_else(diff_syntax::empty_spans);
    }
    if let Some(sequence) = presentation.hunk_sequence(row, cell) {
        return syntax
            .highlight_document_lines(
                sequence.id,
                LanguageHint::Path(sequence.path),
                sequence.lines(),
            )
            .line_shared(sequence.target_line)
            .unwrap_or_else(diff_syntax::empty_spans);
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
    let document = Arc::new(DiffDocument {
        repo_root: String::new(),
        files: vec![file],
    });
    let presentation = DiffPresentation::new(
        document,
        PresentationOptions {
            view_mode: options.view_mode,
            split_when_auto: width >= SPLIT_BREAKPOINT,
            include_file_headers: false,
        },
    );
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
                |cell, width| render_cell(&presentation, row, cell, width, theme, highlighter);
            match presentation.layout() {
                Layout::Unified => row
                    .primary_cell()
                    .map_or_else(Line::default, |cell| cell_line(cell, width)),
                Layout::Split => {
                    let half = width.saturating_sub(1) / 2;
                    let left = row
                        .left
                        .as_ref()
                        .map_or_else(Line::default, |cell| cell_line(cell, half));
                    let right = row
                        .right
                        .as_ref()
                        .map_or_else(Line::default, |cell| cell_line(cell, half));
                    let mut spans = left.spans;
                    spans.push(Span::styled("│", Style::new().fg(color(theme.diff.border))));
                    spans.extend(right.spans);
                    Line::from(spans)
                }
            }
        })
        .collect::<Vec<_>>();
    let overflow = eligible.len().saturating_sub(shown);
    if options.overflow_summary && overflow > 0 {
        lines.push(Line::styled(
            format!("… {overflow} more rows"),
            Style::new().fg(color(theme.diff.muted)),
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
    let base = Style::new()
        .fg(color(colors.foreground))
        .bg(color(colors.background));
    let marker = cell.tone.marker();
    let number = cell
        .line_number()
        .map_or_else(|| "    ".to_owned(), |line| format!("{line:>4}"));
    let prefix = format!("{number} {marker} ");
    let available = usize::from(width).saturating_sub(7);
    let text = truncate_width(&cell.text, available);
    let spans = cell_highlights(highlighter, &theme.syntax, presentation, row, cell);
    let clipped_spans = spans
        .iter()
        .filter_map(|span| clip_span(span, text.len()))
        .collect::<Vec<_>>();
    let mut line = highlighted_line(&text, &clipped_spans, base);
    line.spans.insert(0, Span::styled(prefix, base));
    line
}

fn clip_span(span: &HighlightSpan, source_len: usize) -> Option<HighlightSpan> {
    let start = span.range.start.min(source_len);
    let end = span.range.end.min(source_len);
    (start < end).then_some(HighlightSpan {
        range: start..end,
        foreground: span.foreground,
        font_style: span.font_style,
    })
}

fn truncate_width(source: &str, width: usize) -> String {
    let mut used: usize = 0;
    source
        .chars()
        .take_while(|character| {
            let next = used.saturating_add(character.width().unwrap_or(0));
            if next > width {
                false
            } else {
                used = next;
                true
            }
        })
        .collect()
}

const fn color(value: Rgba) -> Color {
    Color::Rgb(value.r, value.g, value.b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use diff_core::testing::DocumentBuilder;

    #[test]
    fn patch_only_cells_keep_multiline_hunk_context() {
        let document = DocumentBuilder::new()
            .file(
                FileDiff::from_texts("src/a.rs", "", "/* alpha\nbeta\ngamma */\nlet x = 1;\n")
                    .unwrap(),
            )
            .build();
        let presentation = DiffPresentation::new(document, PresentationOptions::default());
        let theme = ReviewTheme::default();
        let mut highlighter = SyntaxHighlighter::default();
        let mut highlights = |text: &str| {
            let (row, cell) = presentation
                .rows(0..presentation.row_count())
                .iter()
                .find_map(|row| {
                    let cell = row.primary_cell()?;
                    (cell.text.as_ref() == text).then_some((row, cell))
                })
                .unwrap();
            cell_highlights(&mut highlighter, &theme.syntax, &presentation, row, cell)
        };

        let opener = highlights("/* alpha");
        let continuation = highlights("beta");
        assert_eq!(continuation.len(), 1, "one comment span covers the line");
        assert_eq!(continuation[0].range, 0.."beta".len());
        assert_eq!(
            continuation[0].foreground, opener[0].foreground,
            "the comment continues across hunk lines"
        );
    }
}
