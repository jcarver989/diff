use crate::annotation::{AnnotationBox, AnnotationKind};
use diff_markdown::{
    MarkdownBlock, MarkdownBlockKind, MarkdownCodeBlock, MarkdownInline, MarkdownReviewSession,
    MarkdownTable, MarkdownTableAlignment, MarkdownTableRow, MarkdownTargetId,
};
use std::collections::HashMap;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkdownTextStyle {
    Plain,
    Heading,
    Strong,
    Emphasis,
    Strikethrough,
    InlineCode,
    Link,
    Image,
    Code,
    Muted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarkdownSpan {
    pub text: String,
    pub style: MarkdownTextStyle,
}

#[derive(Debug, Clone)]
pub(crate) struct MarkdownCodeInfo {
    pub language: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct MarkdownVisualRow {
    pub target: Option<MarkdownTargetId>,
    pub source_line: Option<usize>,
    pub selectable: bool,
    pub spans: Vec<MarkdownSpan>,
    pub prefix: String,
    pub code: Option<MarkdownCodeInfo>,
    pub annotation: Option<(AnnotationBox, usize)>,
}

impl MarkdownVisualRow {
    fn text(
        target: Option<MarkdownTargetId>,
        spans: Vec<MarkdownSpan>,
        prefix: impl Into<String>,
    ) -> Self {
        Self {
            target,
            source_line: None,
            selectable: target.is_some(),
            spans,
            prefix: prefix.into(),
            code: None,
            annotation: None,
        }
    }

    fn with_source_line(mut self, source_line: Option<usize>) -> Self {
        self.source_line = source_line;
        self
    }

    fn spacer() -> Self {
        Self::text(None, Vec::new(), "")
    }

    fn annotation(annotation: AnnotationBox, line: usize) -> Self {
        Self {
            target: None,
            source_line: None,
            selectable: false,
            spans: Vec::new(),
            prefix: String::new(),
            code: None,
            annotation: Some((annotation, line)),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct MarkdownVisualLayout {
    pub rows: Vec<MarkdownVisualRow>,
    pub first_row: HashMap<MarkdownTargetId, usize>,
}

impl MarkdownVisualLayout {
    pub(crate) fn build(session: &MarkdownReviewSession, width: u16) -> Self {
        let document = session.document();
        let mut builder = LayoutBuilder {
            width: usize::from(width.max(1)),
            session,
            rows: Vec::new(),
            first_row: HashMap::new(),
        };
        let mut next_source_line = 1;
        for block in document.blocks() {
            for source_line in next_source_line..block.source.lines.start {
                builder
                    .rows
                    .push(MarkdownVisualRow::spacer().with_source_line(Some(source_line)));
            }
            builder.block(block, None, false);
            next_source_line = block.source.lines.end.saturating_add(1);
        }
        Self {
            rows: builder.rows,
            first_row: builder.first_row,
        }
    }

    pub(crate) fn row_for_target(&self, target: MarkdownTargetId) -> Option<usize> {
        self.first_row.get(&target).copied()
    }
}

struct LayoutBuilder<'a> {
    width: usize,
    session: &'a MarkdownReviewSession,
    rows: Vec<MarkdownVisualRow>,
    first_row: HashMap<MarkdownTargetId, usize>,
}

impl LayoutBuilder<'_> {
    #[expect(
        clippy::too_many_lines,
        reason = "the semantic block dispatch is clearer as one exhaustive match"
    )]
    fn block(
        &mut self,
        block: &MarkdownBlock,
        owner: Option<MarkdownTargetId>,
        suppress_targets: bool,
    ) {
        let target = if suppress_targets {
            owner
        } else {
            block.target_id
        };
        match &block.kind {
            MarkdownBlockKind::Heading { level, content } => {
                let spans = heading_spans(content);
                self.inline_block(
                    target,
                    &spans,
                    format!("{} ", "#".repeat(usize::from(*level))),
                    !suppress_targets,
                    Some(block.source.lines.start),
                );
            }
            MarkdownBlockKind::Paragraph { content } => {
                let spans = inline_spans(content);
                self.inline_block(
                    target,
                    &spans,
                    "",
                    !suppress_targets,
                    Some(block.source.lines.start),
                );
            }
            MarkdownBlockKind::List {
                ordered,
                start,
                items,
            } => {
                for (offset, item) in items.iter().enumerate() {
                    let item_target = if suppress_targets {
                        owner
                    } else {
                        item.target_id
                    };
                    let marker = if *ordered {
                        format!("{}.", start.unwrap_or(1).saturating_add(offset as u64))
                    } else {
                        "•".to_owned()
                    };
                    self.inline_block(
                        item_target,
                        &inline_spans(&item.content),
                        format!("{}{} ", "  ".repeat(item.depth), marker),
                        !suppress_targets,
                        Some(item.source.lines.start),
                    );
                    for child in &item.blocks {
                        self.block(child, item_target, suppress_targets);
                    }
                }
            }
            MarkdownBlockKind::BlockQuote { blocks } => {
                let quote_target = if suppress_targets {
                    owner
                } else {
                    block.target_id
                };
                let start = self.rows.len();
                for child in blocks {
                    self.block(child, quote_target, true);
                }
                if blocks.is_empty() {
                    self.inline_block(
                        quote_target,
                        &[],
                        "│ ",
                        !suppress_targets,
                        Some(block.source.lines.start),
                    );
                }
                for (index, row) in self.rows[start..].iter_mut().enumerate() {
                    row.prefix = format!("│ {}", row.prefix);
                    row.selectable = index == 0 && quote_target.is_some() && !suppress_targets;
                }
                self.mark_target(quote_target, start);
                if !suppress_targets {
                    self.add_annotations(quote_target);
                }
            }
            MarkdownBlockKind::CodeBlock(code) => self.code_block(code, target, suppress_targets),
            MarkdownBlockKind::Table(table) => {
                let widths = table_column_widths(table, self.width);
                for row in &table.rows {
                    let row_target = if suppress_targets {
                        owner
                    } else {
                        row.target_id
                    };
                    self.table_row(
                        row,
                        row_target,
                        !suppress_targets,
                        &widths,
                        &table.alignments,
                    );
                    if row.header {
                        self.rows.push(table_separator(&widths));
                    }
                }
            }
            MarkdownBlockKind::Rule => {
                self.rows.push(
                    MarkdownVisualRow::text(
                        None,
                        vec![MarkdownSpan {
                            text: "─".repeat(self.width),
                            style: MarkdownTextStyle::Muted,
                        }],
                        "",
                    )
                    .with_source_line(Some(block.source.lines.start)),
                );
            }
            MarkdownBlockKind::HtmlFallback { content } => {
                let spans = inline_spans(content);
                self.inline_block(None, &spans, "", false, Some(block.source.lines.start));
            }
        }
    }

    fn inline_block(
        &mut self,
        target: Option<MarkdownTargetId>,
        spans: &[MarkdownSpan],
        prefix: impl Into<String>,
        annotate: bool,
        source_line: Option<usize>,
    ) {
        let prefix = prefix.into();
        let prefix_width = prefix.width();
        let wrapped = wrap_spans(spans, self.width.saturating_sub(prefix_width).max(1));
        let start = self.rows.len();
        if wrapped.is_empty() {
            self.rows
                .push(MarkdownVisualRow::text(target, Vec::new(), prefix));
        } else {
            for (index, line) in wrapped.into_iter().enumerate() {
                self.rows.push(MarkdownVisualRow::text(
                    target,
                    line,
                    if index == 0 {
                        prefix.clone()
                    } else {
                        " ".repeat(prefix_width)
                    },
                ));
            }
        }
        if let Some(row) = self.rows.get_mut(start) {
            row.source_line = source_line;
        }
        self.mark_target(target, start);
        if annotate {
            self.add_annotations(target);
        }
    }

    fn code_block(
        &mut self,
        code: &MarkdownCodeBlock,
        target: Option<MarkdownTargetId>,
        suppress_targets: bool,
    ) {
        let start = self.rows.len();
        let header = code.language.as_deref().map_or_else(
            || "Code".to_owned(),
            |language| format!("Code · {language}"),
        );
        self.rows.push(
            MarkdownVisualRow::text(
                target,
                vec![MarkdownSpan {
                    text: header,
                    style: MarkdownTextStyle::Muted,
                }],
                "  ",
            )
            .with_source_line(Some(code.source.lines.start)),
        );
        self.mark_target(target, start);
        for line in &code.lines {
            let line_target = if suppress_targets {
                target
            } else {
                line.target_id
            };
            let prefix = String::new();
            let available = self.width.saturating_sub(prefix.width()).max(1);
            let spans = vec![MarkdownSpan {
                text: line.text.clone(),
                style: MarkdownTextStyle::Code,
            }];
            let wrapped = wrap_spans(&spans, available);
            let row_start = self.rows.len();
            if wrapped.is_empty() {
                self.rows.push(MarkdownVisualRow {
                    target: line_target,
                    source_line: line.source_line,
                    selectable: line_target.is_some(),
                    spans: Vec::new(),
                    prefix: prefix.clone(),
                    code: Some(MarkdownCodeInfo {
                        language: code.language.clone(),
                    }),
                    annotation: None,
                });
            } else {
                for (index, spans) in wrapped.into_iter().enumerate() {
                    self.rows.push(MarkdownVisualRow {
                        target: line_target,
                        source_line: (index == 0).then_some(line.source_line).flatten(),
                        selectable: line_target.is_some() && index == 0,
                        spans,
                        prefix: if index == 0 {
                            prefix.clone()
                        } else {
                            " ".repeat(prefix.width())
                        },
                        code: Some(MarkdownCodeInfo {
                            language: code.language.clone(),
                        }),
                        annotation: None,
                    });
                }
            }
            if let Some(line_target) = line_target {
                self.first_row.entry(line_target).or_insert(row_start);
            }
            if !suppress_targets {
                self.add_annotations(line_target);
            }
        }
        if !suppress_targets {
            self.add_annotations(target);
        }
    }

    fn table_row(
        &mut self,
        row: &MarkdownTableRow,
        target: Option<MarkdownTargetId>,
        annotate: bool,
        widths: &[usize],
        alignments: &[MarkdownTableAlignment],
    ) {
        let wrapped_cells = row
            .cells
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                let mut spans = inline_spans(&cell.content);
                if row.header {
                    for span in &mut spans {
                        if span.style == MarkdownTextStyle::Plain {
                            span.style = MarkdownTextStyle::Strong;
                        }
                    }
                }
                wrap_spans(&spans, widths.get(index).copied().unwrap_or(1))
            })
            .collect::<Vec<_>>();
        let height = wrapped_cells.iter().map(Vec::len).max().unwrap_or(1);
        let start = self.rows.len();
        for line_index in 0..height {
            let mut spans = Vec::new();
            for (column, lines) in wrapped_cells.iter().enumerate() {
                if column > 0 {
                    push_span(&mut spans, " │ ", MarkdownTextStyle::Muted);
                }
                let cell = lines.get(line_index).cloned().unwrap_or_default();
                spans.extend(align_spans(
                    cell,
                    widths.get(column).copied().unwrap_or(1),
                    table_alignment(column, alignments),
                ));
            }
            push_span(&mut spans, " │", MarkdownTextStyle::Muted);
            let mut visual = MarkdownVisualRow::text(target, spans, "│ ");
            visual.source_line = (line_index == 0).then_some(row.source.lines.start);
            visual.selectable = target.is_some() && line_index == 0;
            self.rows.push(visual);
        }
        self.mark_target(target, start);
        if annotate {
            self.add_annotations(target);
        }
    }

    fn mark_target(&mut self, target: Option<MarkdownTargetId>, row: usize) {
        if let Some(target) = target {
            self.first_row.entry(target).or_insert(row);
            if let Some(visual) = self.rows.get_mut(row) {
                visual.selectable = true;
            }
        }
    }

    fn add_annotations(&mut self, target: Option<MarkdownTargetId>) {
        let Some(target) = target else { return };
        let annotation_width = u16::try_from(self.width).unwrap_or(u16::MAX);
        let comments = self
            .session
            .review()
            .comments_for_target(self.session.document(), target)
            .collect::<Vec<_>>();
        for comment in comments {
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
            let annotation = AnnotationBox::new(kind, title, &comment.body, None, annotation_width);
            let lines = annotation.lines().len();
            for line in 0..lines {
                self.rows
                    .push(MarkdownVisualRow::annotation(annotation.clone(), line));
            }
        }
        if let Some(draft) = self
            .session
            .draft()
            .filter(|draft| draft.target() == target)
        {
            let annotation = AnnotationBox::new(
                AnnotationKind::Draft,
                "Draft",
                draft.body(),
                Some(draft.cursor()),
                annotation_width,
            );
            for line in 0..annotation.lines().len() {
                self.rows
                    .push(MarkdownVisualRow::annotation(annotation.clone(), line));
            }
        }
    }
}

fn inline_spans(inlines: &[MarkdownInline]) -> Vec<MarkdownSpan> {
    let mut spans = Vec::new();
    for inline in inlines {
        inline_span(inline, MarkdownTextStyle::Plain, &mut spans);
    }
    spans
}

fn heading_spans(inlines: &[MarkdownInline]) -> Vec<MarkdownSpan> {
    let mut spans = inline_spans(inlines);
    for span in &mut spans {
        if span.style == MarkdownTextStyle::Plain {
            span.style = MarkdownTextStyle::Heading;
        }
    }
    spans
}

fn inline_span(inline: &MarkdownInline, style: MarkdownTextStyle, output: &mut Vec<MarkdownSpan>) {
    match inline {
        MarkdownInline::Text(text) => push_span(output, text, style),
        MarkdownInline::Code(text) => push_span(output, text, MarkdownTextStyle::InlineCode),
        MarkdownInline::Strong(children) => children
            .iter()
            .for_each(|child| inline_span(child, MarkdownTextStyle::Strong, output)),
        MarkdownInline::Emphasis(children) => children
            .iter()
            .for_each(|child| inline_span(child, MarkdownTextStyle::Emphasis, output)),
        MarkdownInline::Strikethrough(children) => children
            .iter()
            .for_each(|child| inline_span(child, MarkdownTextStyle::Strikethrough, output)),
        MarkdownInline::Link { content, .. } => content
            .iter()
            .for_each(|child| inline_span(child, MarkdownTextStyle::Link, output)),
        MarkdownInline::SoftBreak => push_span(output, " ", style),
        MarkdownInline::HardBreak => push_span(output, "\n", style),
        MarkdownInline::ImageAlt(text) => {
            push_span(output, &format!("Image: {text}"), MarkdownTextStyle::Image);
        }
    }
}

fn push_span(output: &mut Vec<MarkdownSpan>, text: &str, style: MarkdownTextStyle) {
    if text.is_empty() {
        return;
    }
    output.push(MarkdownSpan {
        text: text.to_owned(),
        style,
    });
}

fn wrap_spans(spans: &[MarkdownSpan], width: usize) -> Vec<Vec<MarkdownSpan>> {
    let width = width.max(1);
    let mut rows: Vec<Vec<MarkdownSpan>> = vec![Vec::new()];
    let mut used: usize = 0;
    for span in spans {
        for character in span.text.chars() {
            if character == '\n' {
                rows.push(Vec::new());
                used = 0;
                continue;
            }
            let character_width = character.width().unwrap_or(0);
            if used > 0 && used.saturating_add(character_width) > width {
                rows.push(Vec::new());
                used = 0;
            }
            if rows.last().is_some_and(|row| {
                row.last()
                    .is_some_and(|last: &MarkdownSpan| last.style == span.style)
            }) {
                if let Some(last) = rows.last_mut().and_then(|row| row.last_mut()) {
                    last.text.push(character);
                }
            } else if let Some(row) = rows.last_mut() {
                row.push(MarkdownSpan {
                    text: character.to_string(),
                    style: span.style,
                });
            }
            used = used.saturating_add(character_width);
        }
    }
    rows
}

fn table_column_widths(table: &MarkdownTable, available_width: usize) -> Vec<usize> {
    let column_count = table
        .rows
        .iter()
        .map(|row| row.cells.len())
        .max()
        .unwrap_or(0);
    if column_count == 0 {
        return Vec::new();
    }

    // Account for the outer borders and the three characters between columns.
    let border_width = 4usize.saturating_add(3usize.saturating_mul(column_count.saturating_sub(1)));
    let content_width = available_width
        .saturating_sub(border_width)
        .max(column_count);
    let base = content_width / column_count;
    let remainder = content_width % column_count;
    (0..column_count)
        .map(|column| base.saturating_add(usize::from(column < remainder)).max(1))
        .collect()
}

fn table_separator(widths: &[usize]) -> MarkdownVisualRow {
    let mut text = String::from("├─");
    for (index, width) in widths.iter().enumerate() {
        if index > 0 {
            text.push_str("─┼─");
        }
        text.push_str(&"─".repeat(*width));
    }
    text.push_str("─┤");
    MarkdownVisualRow::text(
        None,
        vec![MarkdownSpan {
            text,
            style: MarkdownTextStyle::Muted,
        }],
        "",
    )
}

fn table_alignment(column: usize, alignments: &[MarkdownTableAlignment]) -> MarkdownTableAlignment {
    alignments
        .get(column)
        .copied()
        .unwrap_or(MarkdownTableAlignment::None)
}

fn align_spans(
    mut spans: Vec<MarkdownSpan>,
    width: usize,
    alignment: MarkdownTableAlignment,
) -> Vec<MarkdownSpan> {
    let used = spans.iter().map(|span| span.text.width()).sum::<usize>();
    let padding = width.saturating_sub(used);
    let (left, right) = match alignment {
        MarkdownTableAlignment::Right => (padding, 0),
        MarkdownTableAlignment::Center => (padding / 2, padding - padding / 2),
        MarkdownTableAlignment::None | MarkdownTableAlignment::Left => (0, padding),
    };
    if left > 0 {
        spans.insert(
            0,
            MarkdownSpan {
                text: " ".repeat(left),
                style: MarkdownTextStyle::Plain,
            },
        );
    }
    if right > 0 {
        spans.push(MarkdownSpan {
            text: " ".repeat(right),
            style: MarkdownTextStyle::Plain,
        });
    }
    spans
}
