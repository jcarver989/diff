use clankerdiff_core::{Fingerprint, SourceSequenceId, join_lines};
use clankerdiff_markdown::{
    MarkdownBlock, MarkdownBlockKind, MarkdownCodeBlock, MarkdownInline, MarkdownListItem, MarkdownTable,
    MarkdownTableAlignment, MarkdownTargetId,
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(super) struct DocumentRow {
    pub target: Option<MarkdownTargetId>,
    pub depth: u16,
    pub gap: f32,
    pub kind: RowKind,
}

impl DocumentRow {
    const fn new(target: Option<MarkdownTargetId>, depth: u16, kind: RowKind) -> Self {
        Self { target, depth, gap: 0.0, kind }
    }
}

#[derive(Debug, Clone)]
pub(super) enum RowKind {
    Heading {
        level: u8,
        content: Vec<MarkdownInline>,
    },
    Paragraph {
        content: Vec<MarkdownInline>,
    },
    ListItem {
        marker: String,
        content: Vec<MarkdownInline>,
    },
    Quote {
        rows: Vec<DocumentRow>,
    },
    CodeHeader {
        code: Arc<CodeSource>,
    },
    CodeLine {
        code: Arc<CodeSource>,
        index: usize,
    },
    TableRow {
        cells: Vec<Vec<MarkdownInline>>,
        alignments: Arc<[MarkdownTableAlignment]>,
        header: bool,
        first: bool,
        last: bool,
    },
    Rule,
    Html {
        content: Vec<MarkdownInline>,
    },
}

#[derive(Debug)]
pub(super) struct CodeSource {
    pub language: Option<String>,
    pub info: Arc<str>,
    pub lines: Vec<String>,
    pub text: Arc<str>,
    pub source_id: Fingerprint,
}

pub(super) fn document_rows(blocks: &[MarkdownBlock]) -> Vec<DocumentRow> {
    let mut rows = Vec::new();
    for block in blocks {
        push_block(block, 0, &mut rows);
    }
    for index in 1..rows.len() {
        rows[index].gap = gap(&rows[index - 1].kind, &rows[index]);
    }
    rows
}

fn push_block(block: &MarkdownBlock, depth: u16, rows: &mut Vec<DocumentRow>) {
    let kind = match &block.kind {
        MarkdownBlockKind::Heading { level, content } => RowKind::Heading { level: *level, content: content.clone() },
        MarkdownBlockKind::Paragraph { content } => RowKind::Paragraph { content: content.clone() },
        MarkdownBlockKind::BlockQuote { blocks } => RowKind::Quote { rows: document_rows(blocks) },
        MarkdownBlockKind::HtmlFallback { content } => RowKind::Html { content: content.clone() },
        MarkdownBlockKind::Rule => RowKind::Rule,
        MarkdownBlockKind::List { ordered, start, items } => return push_list(*ordered, *start, items, depth, rows),
        MarkdownBlockKind::CodeBlock(code) => return push_code(code, depth, rows),
        MarkdownBlockKind::Table(table) => return push_table(table, depth, rows),
    };
    rows.push(DocumentRow::new(block.target_id, depth, kind));
}

fn push_list(ordered: bool, start: Option<u64>, items: &[MarkdownListItem], depth: u16, rows: &mut Vec<DocumentRow>) {
    let first = start.unwrap_or(1);
    for (offset, item) in (0_u64..).zip(items) {
        let marker = if ordered { format!("{}.", first + offset) } else { bullet(depth).to_owned() };
        let content = item.content.clone();
        rows.push(DocumentRow::new(item.target_id, depth, RowKind::ListItem { marker, content }));
        for block in &item.blocks {
            push_block(block, depth.saturating_add(1), rows);
        }
    }
}

fn push_code(code: &MarkdownCodeBlock, depth: u16, rows: &mut Vec<DocumentRow>) {
    let lines: Vec<String> = code.lines.iter().map(|line| line.text.clone()).collect();
    let code_source = Arc::new(CodeSource {
        language: code.language.clone(),
        info: Arc::from(code.highlight_hint()),
        text: Arc::from(join_lines(lines.iter().map(String::as_str))),
        source_id: SourceSequenceId::from_lines(lines.iter().map(String::as_str)).fingerprint(),
        lines,
    });
    rows.push(DocumentRow::new(code.target_id, depth, RowKind::CodeHeader { code: Arc::clone(&code_source) }));
    for line in &code.lines {
        rows.push(DocumentRow::new(
            line.target_id,
            depth,
            RowKind::CodeLine { code: Arc::clone(&code_source), index: line.index },
        ));
    }
}

fn push_table(table: &MarkdownTable, depth: u16, rows: &mut Vec<DocumentRow>) {
    let alignments: Arc<[MarkdownTableAlignment]> = Arc::from(table.alignments.as_slice());
    let count = table.rows.len();
    for (index, row) in table.rows.iter().enumerate() {
        rows.push(DocumentRow::new(
            row.target_id,
            depth,
            RowKind::TableRow {
                cells: row.cells.iter().map(|cell| cell.content.clone()).collect(),
                alignments: Arc::clone(&alignments),
                header: row.header,
                first: index == 0,
                last: index + 1 == count,
            },
        ));
    }
}

fn gap(previous: &RowKind, row: &DocumentRow) -> f32 {
    match (&row.kind, previous) {
        (RowKind::CodeLine { .. } | RowKind::TableRow { first: false, .. }, _) => 0.0,
        (RowKind::Heading { level: 1, .. }, _) => 36.0,
        (RowKind::Heading { level: 2, .. }, _) => 32.0,
        (RowKind::Heading { .. }, _) => 24.0,
        (_, RowKind::Heading { .. }) => 8.0,
        (RowKind::ListItem { .. }, RowKind::ListItem { .. }) => 4.0,
        (_, _) if row.depth > 0 => 6.0,
        (RowKind::Rule, _) => 20.0,
        _ => 14.0,
    }
}

const fn bullet(depth: u16) -> &'static str {
    match depth % 3 {
        0 => "•",
        1 => "◦",
        _ => "▪",
    }
}
