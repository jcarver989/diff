use diff_core::{CommentDraft, ReviewComment, ReviewSession};
use std::collections::BTreeMap;
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
    pub(crate) fn comment(comment: &ReviewComment, available_width: u16) -> Self {
        let kind = if comment.outdated {
            AnnotationKind::Outdated
        } else {
            AnnotationKind::Comment
        };
        let title = if comment.outdated {
            "Outdated comment"
        } else {
            "Comment"
        };
        Self::new(kind, title, &comment.body, None, available_width)
    }

    pub(crate) fn draft(draft: &CommentDraft, available_width: u16) -> Self {
        Self::new(
            AnnotationKind::Draft,
            "Draft",
            draft.body(),
            Some(draft.cursor()),
            available_width,
        )
    }

    fn new(
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
                compact_width,
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

#[derive(Debug, Clone)]
pub(crate) enum PatchVisualRow<'a> {
    Source(usize),
    Annotation {
        source: usize,
        annotation: &'a AnnotationBox,
        line: usize,
    },
}

#[derive(Debug, Default)]
pub(crate) struct PatchVisualLayout {
    range_start: usize,
    source_rows: usize,
    annotations: Vec<AnchoredAnnotations>,
    total_rows: usize,
}

#[derive(Debug)]
struct AnchoredAnnotations {
    source: usize,
    boxes: Vec<AnnotationBox>,
    extra_rows: usize,
}

impl PatchVisualLayout {
    pub(crate) fn new(session: &ReviewSession, range: std::ops::Range<usize>, width: u16) -> Self {
        let mut anchored: BTreeMap<usize, Vec<AnnotationBox>> = BTreeMap::new();
        for comment in session.review().comments() {
            if let Some(row) = session
                .presentation()
                .row_showing_anchor(&comment.anchor)
                .filter(|row| range.contains(row))
            {
                anchored
                    .entry(row)
                    .or_default()
                    .push(AnnotationBox::comment(comment, width));
            }
        }
        if let Some(draft) = session.draft()
            && let Some(row) = session
                .presentation()
                .row_showing_anchor(draft.anchor())
                .filter(|row| range.contains(row))
        {
            anchored
                .entry(row)
                .or_default()
                .push(AnnotationBox::draft(draft, width));
        }
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

    pub(crate) fn row(&self, visual: usize) -> Option<PatchVisualRow<'_>> {
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
                return Some(PatchVisualRow::Source(
                    self.range_start + visual.saturating_sub(extras),
                ));
            }
            if visual == source_offset {
                return Some(PatchVisualRow::Source(annotation.source));
            }
            if visual <= source_offset.saturating_add(annotation.extra_rows) {
                let mut line = visual - source_offset - 1;
                for entry in &annotation.boxes {
                    if line < entry.lines.len() {
                        return Some(PatchVisualRow::Annotation {
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
        Some(PatchVisualRow::Source(
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

fn fit_to_width(text: &str, width: u16) -> String {
    let width = usize::from(width);
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
}
