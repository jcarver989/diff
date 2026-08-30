//! Renderer-neutral Ratatui annotation layout primitives.
//!
//! This module deliberately knows nothing about diff reviews. Diff-specific
//! comments are converted to [`AnnotationBox`] values by `patch_layout`.

use crate::RatatuiTheme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use std::collections::BTreeMap;
use std::ops::Range;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const LEFT_INDENT: u16 = 2;
const RIGHT_INSET: u16 = 1;
const HORIZONTAL_PADDING: u16 = 1;
const MIN_BOX_WIDTH: u16 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnnotationKind {
    Comment,
    Outdated,
    Draft,
}

#[derive(Debug, Clone)]
pub(crate) enum AnnotationLine {
    Top,
    Body(String),
    Bottom,
    Compact(String),
}

/// The terminal representation of one saved comment or active draft.
#[derive(Debug, Clone)]
pub(crate) struct AnnotationBox {
    kind: AnnotationKind,
    title: String,
    width: u16,
    indent: u16,
    lines: Vec<AnnotationLine>,
    cursor: Option<(usize, u16)>,
}

impl AnnotationBox {
    /// Lays out an annotation without depending on a particular review model.
    pub(crate) fn new(
        kind: AnnotationKind,
        title: &str,
        body: &str,
        cursor: Option<usize>,
        available_width: u16,
    ) -> Self {
        let indent = LEFT_INDENT.min(available_width);
        let width = available_width
            .saturating_sub(indent)
            .saturating_sub(RIGHT_INSET);
        if width < MIN_BOX_WIDTH {
            let compact_width = available_width.saturating_sub(indent);
            let prefix = format!("│ {title}: ");
            let text = fit_to_width(
                &format!("{prefix}{}", body.replace('\n', " ⏎ ")),
                usize::from(compact_width),
            );
            let cursor = cursor.map(|cursor| {
                let before = format!("{prefix}{}", body[..cursor].replace('\n', " ⏎ "));
                (
                    0,
                    indent.saturating_add(
                        display_width(&before).min(compact_width.saturating_sub(1)),
                    ),
                )
            });
            return Self {
                kind,
                title: title.to_owned(),
                width: compact_width,
                indent,
                lines: vec![AnnotationLine::Compact(text)],
                cursor,
            };
        }

        let body_width = width
            .saturating_sub(2)
            .saturating_sub(HORIZONTAL_PADDING.saturating_mul(2));
        let (body_lines, wrapped_cursor) = wrap_with_cursor(body, usize::from(body_width), cursor);
        let mut lines = Vec::with_capacity(body_lines.len() + 2);
        lines.push(AnnotationLine::Top);
        lines.extend(body_lines.into_iter().map(AnnotationLine::Body));
        lines.push(AnnotationLine::Bottom);
        let cursor = wrapped_cursor.map(|(row, column)| {
            (
                row + 1,
                indent
                    .saturating_add(1)
                    .saturating_add(HORIZONTAL_PADDING)
                    .saturating_add(u16::try_from(column).unwrap_or(u16::MAX)),
            )
        });
        Self {
            kind,
            title: title.to_owned(),
            width,
            indent,
            lines,
            cursor,
        }
    }

    pub(crate) const fn kind(&self) -> AnnotationKind {
        self.kind
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    pub(crate) const fn width(&self) -> u16 {
        self.width
    }

    pub(crate) const fn indent(&self) -> u16 {
        self.indent
    }

    pub(crate) fn lines(&self) -> &[AnnotationLine] {
        &self.lines
    }

    pub(crate) fn cursor_column(&self, line: usize) -> Option<u16> {
        self.cursor
            .filter(|(cursor_line, _)| *cursor_line == line)
            .map(|(_, column)| column)
    }
}

/// A source row or an annotation row inserted immediately after a source row.
#[derive(Debug, Clone)]
pub(crate) enum AnnotationRow<'a> {
    Source(usize),
    Annotation {
        source: usize,
        annotation: &'a AnnotationBox,
        line: usize,
    },
}

#[derive(Debug)]
struct AnchoredAnnotations {
    source: usize,
    boxes: Vec<AnnotationBox>,
    extra_rows: usize,
}

/// Inserts laid-out annotation boxes into a source-row sequence.
///
/// This is intentionally independent of review comments and anchors. A
/// renderer can provide any source-row index and any number of boxes at that
/// index, while preserving the invariant that boxes are rendered after their
/// source row.
#[derive(Debug, Default)]
pub(crate) struct AnnotationLayout {
    range_start: usize,
    source_rows: usize,
    annotations: Vec<AnchoredAnnotations>,
    total_rows: usize,
}

impl AnnotationLayout {
    pub(crate) fn new(range: Range<usize>, anchored: BTreeMap<usize, Vec<AnnotationBox>>) -> Self {
        let annotations: Vec<_> = anchored
            .into_iter()
            .map(|(source, boxes)| AnchoredAnnotations {
                source,
                extra_rows: boxes.iter().map(|annotation| annotation.lines.len()).sum(),
                boxes,
            })
            .collect();
        let source_rows = range.len();
        let total_rows = source_rows.saturating_add(
            annotations
                .iter()
                .map(|annotation| annotation.extra_rows)
                .sum::<usize>(),
        );
        Self {
            range_start: range.start,
            source_rows,
            annotations,
            total_rows,
        }
    }

    pub(crate) const fn len(&self) -> usize {
        self.total_rows
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.total_rows == 0
    }

    pub(crate) fn visual_offset_for_source(&self, source: usize) -> Option<usize> {
        let relative = source.checked_sub(self.range_start)?;
        if relative >= self.source_rows {
            return None;
        }
        let extras = self
            .annotations
            .iter()
            .take_while(|annotation| annotation.source < source)
            .map(|annotation| annotation.extra_rows)
            .sum::<usize>();
        Some(relative.saturating_add(extras))
    }

    pub(crate) fn focused_visual_row(&self, source: usize, draft: bool) -> Option<usize> {
        let source_offset = self.visual_offset_for_source(source)?;
        if !draft {
            return Some(source_offset);
        }
        let annotation = self
            .annotations
            .iter()
            .find(|annotation| annotation.source == source)?;
        let mut offset = source_offset + 1;
        for entry in &annotation.boxes {
            if entry.kind == AnnotationKind::Draft {
                let cursor_line = entry.cursor.map_or(1, |(line, _)| line);
                return Some(offset + cursor_line);
            }
            offset += entry.lines.len();
        }
        Some(source_offset)
    }

    pub(crate) fn row(&self, visual: usize) -> Option<AnnotationRow<'_>> {
        if visual >= self.total_rows {
            return None;
        }
        let mut extras = 0;
        for annotation in &self.annotations {
            let source_offset = annotation
                .source
                .saturating_sub(self.range_start)
                .saturating_add(extras);
            if visual < source_offset {
                return Some(AnnotationRow::Source(
                    self.range_start + visual.saturating_sub(extras),
                ));
            }
            if visual == source_offset {
                return Some(AnnotationRow::Source(annotation.source));
            }
            if visual <= source_offset.saturating_add(annotation.extra_rows) {
                let mut line = visual - source_offset - 1;
                for entry in &annotation.boxes {
                    if line < entry.lines.len() {
                        return Some(AnnotationRow::Annotation {
                            source: annotation.source,
                            annotation: entry,
                            line,
                        });
                    }
                    line -= entry.lines.len();
                }
            }
            extras = extras.saturating_add(annotation.extra_rows);
        }
        Some(AnnotationRow::Source(
            self.range_start + visual.saturating_sub(extras),
        ))
    }
}

fn wrap_with_cursor(
    text: &str,
    width: usize,
    cursor: Option<usize>,
) -> (Vec<String>, Option<(usize, usize)>) {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut line_width = 0_usize;
    let mut cursor_position = None;

    for (offset, character) in text.char_indices() {
        let character_width = character.width().unwrap_or(0);
        if character != '\n' && line_width > 0 && line_width.saturating_add(character_width) > width
        {
            lines.push(std::mem::take(&mut line));
            line_width = 0;
        }
        if cursor == Some(offset) {
            cursor_position = Some((lines.len(), line_width));
        }
        if character == '\n' {
            lines.push(std::mem::take(&mut line));
            line_width = 0;
        } else {
            line.push(character);
            line_width = line_width.saturating_add(character_width);
        }
    }
    if cursor == Some(text.len()) && line_width == width {
        lines.push(std::mem::take(&mut line));
        line_width = 0;
    }
    if cursor == Some(text.len()) {
        cursor_position = Some((lines.len(), line_width));
    }
    lines.push(line);
    (lines, cursor_position)
}

fn fit_to_width(text: &str, width: usize) -> String {
    let mut used = 0_usize;
    text.chars()
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

fn display_width(text: &str) -> u16 {
    u16::try_from(text.width()).unwrap_or(u16::MAX)
}

/// Renders one laid-out annotation row using the shared Ratatui theme.
pub(crate) fn render_annotation_line(
    area: Rect,
    buffer: &mut Buffer,
    theme: &RatatuiTheme,
    annotation: &AnnotationBox,
    line: usize,
) {
    let Some(line) = annotation.lines().get(line) else {
        return;
    };
    let border_color = match annotation.kind() {
        AnnotationKind::Outdated => theme.ui.text_muted,
        AnnotationKind::Comment | AnnotationKind::Draft => theme.ui.accent,
    };
    let body_color = if annotation.kind() == AnnotationKind::Outdated {
        theme.ui.text_muted
    } else {
        theme.ui.text
    };
    let border = Style::new().fg(border_color).bg(theme.ui.canvas);
    let body = Style::new().fg(body_color).bg(theme.ui.canvas);
    let box_area = Rect::new(
        area.x.saturating_add(annotation.indent()),
        area.y,
        annotation
            .width()
            .min(area.width.saturating_sub(annotation.indent())),
        1,
    );
    let width = usize::from(box_area.width);
    if width == 0 {
        return;
    }
    let rendered = match line {
        AnnotationLine::Top => {
            let available = width.saturating_sub(2);
            let label = format!("─ {} ", annotation.title());
            let label = fit_to_width(&label, available);
            let fill = "─".repeat(available.saturating_sub(label.width()));
            Line::from(vec![
                Span::styled("╭", border),
                Span::styled(
                    label,
                    border.add_modifier(if annotation.kind() == AnnotationKind::Draft {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                ),
                Span::styled(fill, border),
                Span::styled("╮", border),
            ])
        }
        AnnotationLine::Body(text) => {
            let body_width = width.saturating_sub(4);
            let fill = " ".repeat(body_width.saturating_sub(text.width()));
            Line::from(vec![
                Span::styled("│ ", border),
                Span::styled(text.clone(), body),
                Span::styled(fill, body),
                Span::styled(" │", border),
            ])
        }
        AnnotationLine::Bottom => {
            Line::styled(format!("╰{}╯", "─".repeat(width.saturating_sub(2))), border)
        }
        AnnotationLine::Compact(text) => Line::from(vec![
            Span::styled("│ ", border),
            Span::styled(text.strip_prefix("│ ").unwrap_or(text).to_owned(), body),
        ]),
    };
    Paragraph::new(rendered).render(box_area, buffer);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_wide_characters_and_maps_the_cursor() {
        let (lines, cursor) = wrap_with_cursor("ab界cd", 4, Some(5));
        assert_eq!(lines, ["ab界", "cd"]);
        assert_eq!(cursor, Some((1, 0)));
    }

    #[test]
    fn an_eof_cursor_after_a_full_row_moves_to_a_continuation_row() {
        let (lines, cursor) = wrap_with_cursor("abcd", 4, Some(4));
        assert_eq!(lines, ["abcd", ""]);
        assert_eq!(cursor, Some((1, 0)));
    }

    #[test]
    fn preserves_explicit_blank_lines() {
        let (lines, _) = wrap_with_cursor("first\n\nlast", 20, None);
        assert_eq!(lines, ["first", "", "last"]);
    }

    #[test]
    fn very_narrow_areas_use_a_compact_line() {
        let box_layout = AnnotationBox::new(
            AnnotationKind::Draft,
            "Draft",
            "界界界",
            Some("界界界".len()),
            8,
        );
        assert!(matches!(
            box_layout.lines.as_slice(),
            [AnnotationLine::Compact(_)]
        ));
        assert!(box_layout.cursor_column(0).is_some());
    }

    #[test]
    fn box_has_horizontal_padding_and_border_rows() {
        let box_layout = AnnotationBox::new(AnnotationKind::Comment, "Comment", "body", None, 30);
        assert!(matches!(box_layout.lines[0], AnnotationLine::Top));
        assert!(matches!(box_layout.lines[1], AnnotationLine::Body(ref body) if body == "body"));
        assert!(matches!(box_layout.lines[2], AnnotationLine::Bottom));
        assert_eq!(box_layout.indent, LEFT_INDENT);
    }

    #[test]
    fn inserts_annotations_after_their_source_rows() {
        let mut anchored = BTreeMap::new();
        anchored.insert(
            1,
            vec![AnnotationBox::new(
                AnnotationKind::Comment,
                "Comment",
                "body",
                None,
                30,
            )],
        );
        let layout = AnnotationLayout::new(0..3, anchored);
        assert!(matches!(layout.row(0), Some(AnnotationRow::Source(0))));
        assert!(matches!(layout.row(1), Some(AnnotationRow::Source(1))));
        assert!(matches!(
            layout.row(2),
            Some(AnnotationRow::Annotation {
                source: 1,
                line: 0,
                ..
            })
        ));
        assert!(matches!(layout.row(5), Some(AnnotationRow::Source(2))));
    }
}
