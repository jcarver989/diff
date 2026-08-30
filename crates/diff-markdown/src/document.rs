use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag};
use serde::{Deserialize, Serialize};
use std::{cmp, ops::Range, sync::Arc};

/// A one-based, inclusive range of lines in the original Markdown source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownLineRange {
    pub start: usize,
    pub end: usize,
}

/// A byte and line range into the original, unmodified source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRange {
    pub bytes: Range<usize>,
    pub lines: MarkdownLineRange,
}

/// An opaque identifier valid only for the lifetime of one parsed document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MarkdownTargetId(usize);

impl MarkdownTargetId {
    /// Returns the document-local ordinal of this target.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// The semantic kinds that can be selected and commented on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarkdownTargetKind {
    Heading,
    Paragraph,
    ListItem,
    BlockQuote,
    TableRow,
    CodeBlock,
    CodeLine,
}

/// A formatted inline Markdown node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkdownInline {
    Text(String),
    Code(String),
    Strong(Vec<MarkdownInline>),
    Emphasis(Vec<MarkdownInline>),
    Strikethrough(Vec<MarkdownInline>),
    Link {
        destination: String,
        title: Option<String>,
        content: Vec<MarkdownInline>,
    },
    SoftBreak,
    HardBreak,
    /// Images intentionally do not load assets. Their alt text is the fallback.
    ImageAlt(String),
}

impl MarkdownInline {
    fn visible_text(&self, output: &mut String) {
        match self {
            Self::Text(text) | Self::Code(text) | Self::ImageAlt(text) => output.push_str(text),
            Self::Strong(children) | Self::Emphasis(children) | Self::Strikethrough(children) => {
                for child in children {
                    child.visible_text(output);
                }
            }
            Self::Link { content, .. } => {
                for child in content {
                    child.visible_text(output);
                }
            }
            Self::SoftBreak | Self::HardBreak => output.push('\n'),
        }
    }
}

/// Parses Markdown into a renderer-neutral semantic document.
#[must_use]
pub fn parse_markdown(source: impl Into<String>) -> MarkdownDocument {
    MarkdownDocument::parse(source)
}

/// Returns the rendered, style-free text represented by inline nodes.
#[must_use]
pub fn rendered_text(inlines: &[MarkdownInline]) -> String {
    let mut text = String::new();
    for inline in inlines {
        inline.visible_text(&mut text);
    }
    text
}

/// A commentable or structurally significant Markdown block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownBlock {
    pub source: SourceRange,
    pub kind: MarkdownBlockKind,
    pub target_id: Option<MarkdownTargetId>,
}

impl MarkdownBlock {
    /// Returns the target ID when this block is commentable.
    #[must_use]
    pub const fn target_id(&self) -> Option<MarkdownTargetId> {
        self.target_id
    }
}

/// The semantic kinds of blocks exposed to renderers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkdownBlockKind {
    Heading {
        level: u8,
        content: Vec<MarkdownInline>,
    },
    Paragraph {
        content: Vec<MarkdownInline>,
    },
    List {
        ordered: bool,
        start: Option<u64>,
        items: Vec<MarkdownListItem>,
    },
    BlockQuote {
        blocks: Vec<MarkdownBlock>,
    },
    CodeBlock(MarkdownCodeBlock),
    Table(MarkdownTable),
    /// Raw HTML is retained as escaped/plain fallback text, but is not a target.
    HtmlFallback {
        content: Vec<MarkdownInline>,
    },
    Rule,
}

/// One list item. The list container itself is not a review target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownListItem {
    pub depth: usize,
    pub source: SourceRange,
    pub content: Vec<MarkdownInline>,
    pub blocks: Vec<MarkdownBlock>,
    pub target_id: Option<MarkdownTargetId>,
}

impl MarkdownListItem {
    /// Returns this item's document-local target ID.
    #[must_use]
    pub const fn target_id(&self) -> Option<MarkdownTargetId> {
        self.target_id
    }
}

/// A fenced or indented code block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownCodeBlock {
    pub language: Option<String>,
    /// The complete info string for a fenced block, when present.
    pub info: Option<String>,
    pub source: SourceRange,
    pub content: SourceRange,
    pub lines: Vec<MarkdownCodeLine>,
    pub target_id: Option<MarkdownTargetId>,
}

impl MarkdownCodeBlock {
    /// Returns this code block's target ID.
    #[must_use]
    pub const fn target_id(&self) -> Option<MarkdownTargetId> {
        self.target_id
    }
}

/// One content line inside a code block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownCodeLine {
    pub index: usize,
    pub source: SourceRange,
    pub source_line: Option<usize>,
    pub text: String,
    pub target_id: Option<MarkdownTargetId>,
}

impl MarkdownCodeLine {
    /// Returns this line's document-local target ID.
    #[must_use]
    pub const fn target_id(&self) -> Option<MarkdownTargetId> {
        self.target_id
    }
}

/// A table and its rows. Cells are structural and are not independent targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownTable {
    pub alignments: Vec<MarkdownTableAlignment>,
    pub rows: Vec<MarkdownTableRow>,
}

/// Alignment requested by a Markdown table delimiter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkdownTableAlignment {
    None,
    Left,
    Center,
    Right,
}

/// One table row, including the header row when one exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownTableRow {
    pub header: bool,
    pub source: SourceRange,
    pub cells: Vec<MarkdownTableCell>,
    pub target_id: Option<MarkdownTargetId>,
}

impl MarkdownTableRow {
    /// Returns this row's document-local target ID.
    #[must_use]
    pub const fn target_id(&self) -> Option<MarkdownTargetId> {
        self.target_id
    }
}

/// One structural table cell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownTableCell {
    pub source: SourceRange,
    pub content: Vec<MarkdownInline>,
}

/// A heading in document outline order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownHeading {
    pub level: u8,
    pub title: String,
    pub source: SourceRange,
    pub target_id: MarkdownTargetId,
}

/// A selectable target in document order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownTarget {
    pub id: MarkdownTargetId,
    pub kind: MarkdownTargetKind,
    pub source: SourceRange,
    pub display_label: String,
}

/// A parsed Markdown document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownDocument {
    source_path: Option<String>,
    title: Option<String>,
    source: Arc<str>,
    blocks: Vec<MarkdownBlock>,
    outline: Vec<MarkdownHeading>,
    targets: Vec<MarkdownTarget>,
}

impl MarkdownDocument {
    /// Creates a document by parsing Markdown with the supported GFM extensions enabled.
    #[must_use]
    pub fn new(source: impl Into<String>) -> Self {
        Self::parse(source)
    }

    /// Returns an empty Markdown document.
    #[must_use]
    pub fn empty() -> Self {
        Self::parse("")
    }

    /// Parses Markdown with the supported GFM extensions enabled.
    #[must_use]
    pub fn parse(source: impl Into<String>) -> Self {
        Self::parse_with_metadata(None, None, source)
    }

    /// Alias for [`Self::parse`] that makes the source-oriented API explicit.
    #[must_use]
    pub fn from_source(source: impl Into<String>) -> Self {
        Self::parse(source)
    }

    /// Parses Markdown while retaining optional host-provided metadata.
    #[must_use]
    pub fn parse_with_metadata(
        source_path: Option<String>,
        title: Option<String>,
        source: impl Into<String>,
    ) -> Self {
        let source: Arc<str> = Arc::from(source.into());
        let line_starts = LineIndex::new(&source);
        let events = Parser::new_ext(&source, parser_options())
            .into_offset_iter()
            .map(|(event, range)| (event.into_static(), range))
            .collect::<Vec<_>>();
        let roots = event_tree(&events);
        let mut blocks = roots
            .iter()
            .filter_map(|node| parse_block(node, 0, &source, &line_starts))
            .collect::<Vec<_>>();
        let mut targets = Vec::new();
        let mut outline = Vec::new();
        assign_targets(&mut blocks, &mut targets, &mut outline);
        Self {
            source_path,
            title,
            source,
            blocks,
            outline,
            targets,
        }
    }

    /// Returns the original source, including its original line endings.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the optional source path supplied by the host.
    #[must_use]
    pub fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }

    /// Returns the optional display title supplied by the host.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Returns the top-level semantic blocks.
    #[must_use]
    pub fn blocks(&self) -> &[MarkdownBlock] {
        &self.blocks
    }

    /// Returns headings in document order.
    #[must_use]
    pub fn outline(&self) -> &[MarkdownHeading] {
        &self.outline
    }

    /// Returns all commentable targets in document order.
    #[must_use]
    pub fn targets(&self) -> &[MarkdownTarget] {
        &self.targets
    }

    /// Finds a target by its document-local ID.
    #[must_use]
    pub fn target(&self, id: MarkdownTargetId) -> Option<&MarkdownTarget> {
        self.targets.get(id.0)
    }
}

#[derive(Debug, Clone)]
struct Node {
    event: Option<Event<'static>>,
    range: Range<usize>,
    children: Vec<Node>,
}

fn parser_options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH
}

fn event_tree(events: &[(Event<'static>, Range<usize>)]) -> Vec<Node> {
    let mut stack = vec![Node {
        event: None,
        range: 0..0,
        children: Vec::new(),
    }];
    for (event, range) in events {
        match event {
            Event::Start(_) => stack.push(Node {
                event: Some(event.clone()),
                range: range.clone(),
                children: Vec::new(),
            }),
            Event::End(_) => {
                if stack.len() > 1
                    && let Some(mut node) = stack.pop()
                {
                    node.range.end = cmp::max(node.range.end, range.end);
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    }
                }
            }
            _ => {
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(Node {
                        event: Some(event.clone()),
                        range: range.clone(),
                        children: Vec::new(),
                    });
                }
            }
        }
    }
    // pulldown-cmark promises balanced events. Recovering here still makes the
    // model useful if a future parser extension violates that promise.
    while stack.len() > 1 {
        if let Some(node) = stack.pop()
            && let Some(parent) = stack.last_mut()
        {
            parent.children.push(node);
        }
    }
    stack.pop().map_or_else(Vec::new, |root| root.children)
}

fn source_range(range: Range<usize>, index: &LineIndex) -> SourceRange {
    SourceRange {
        lines: index.range(range.clone()),
        bytes: range,
    }
}

fn parse_block(
    node: &Node,
    depth: usize,
    source: &str,
    line_index: &LineIndex,
) -> Option<MarkdownBlock> {
    let event = node.event.as_ref()?;
    let kind = match event {
        Event::Start(Tag::Heading { level, .. }) => MarkdownBlockKind::Heading {
            level: heading_level(*level),
            content: inline_children(&node.children),
        },
        Event::Start(Tag::Paragraph) => MarkdownBlockKind::Paragraph {
            content: inline_children(&node.children),
        },
        Event::Start(Tag::BlockQuote(_)) => MarkdownBlockKind::BlockQuote {
            blocks: node
                .children
                .iter()
                .filter_map(|child| parse_block(child, depth, source, line_index))
                .collect(),
        },
        Event::Start(Tag::List(start)) => MarkdownBlockKind::List {
            ordered: start.is_some(),
            start: *start,
            items: node
                .children
                .iter()
                .filter_map(|child| parse_item(child, depth, source, line_index))
                .collect(),
        },
        Event::Start(Tag::CodeBlock(kind)) => {
            MarkdownBlockKind::CodeBlock(parse_code_block(node, kind, source, line_index))
        }
        Event::Start(Tag::Table(alignments)) => {
            MarkdownBlockKind::Table(parse_table(node, alignments, line_index))
        }
        Event::Rule => MarkdownBlockKind::Rule,
        Event::Start(Tag::HtmlBlock) => MarkdownBlockKind::HtmlFallback {
            content: html_fallback(&node.children),
        },
        _ => return None,
    };
    Some(MarkdownBlock {
        source: source_range(node.range.clone(), line_index),
        kind,
        target_id: None,
    })
}

fn parse_item(
    node: &Node,
    depth: usize,
    source: &str,
    line_index: &LineIndex,
) -> Option<MarkdownListItem> {
    if !matches!(node.event, Some(Event::Start(Tag::Item))) {
        return None;
    }
    let mut content = Vec::new();
    let mut blocks = Vec::new();
    let mut took_paragraph = false;
    for child in &node.children {
        if !took_paragraph && matches!(child.event, Some(Event::Start(Tag::Paragraph))) {
            content = inline_children(&child.children);
            took_paragraph = true;
        } else if let Some(inline) = inline_node(child) {
            // Tight list items may contain inline events directly rather than
            // a paragraph wrapper. Preserve those events as the item's content.
            content.push(inline);
        } else if let Some(block) = parse_block(child, depth + 1, source, line_index) {
            blocks.push(block);
        }
    }
    Some(MarkdownListItem {
        depth,
        source: source_range(node.range.clone(), line_index),
        content,
        blocks,
        target_id: None,
    })
}

fn parse_code_block(
    node: &Node,
    kind: &CodeBlockKind<'static>,
    source: &str,
    line_index: &LineIndex,
) -> MarkdownCodeBlock {
    let source_range_value = source_range(node.range.clone(), line_index);
    let text = node
        .children
        .iter()
        .filter_map(|child| match child.event.as_ref()? {
            Event::Text(text) => Some(text.as_ref()),
            _ => None,
        })
        .collect::<String>();
    let (info, language) = match kind {
        CodeBlockKind::Indented => (None, None),
        CodeBlockKind::Fenced(info) => {
            let info = info.to_string();
            let language = info.split_whitespace().next().map(str::to_owned);
            (Some(info), language)
        }
    };
    let content_bytes = code_content_range(node.range.clone(), kind, source);
    let fenced = matches!(kind, CodeBlockKind::Fenced(_));
    let code = if fenced {
        source
            .get(content_bytes.clone())
            .unwrap_or(text.as_str())
            .to_owned()
    } else {
        text.strip_suffix('\n').unwrap_or(&text).to_owned()
    };
    let first_source_line = line_index.line_for_offset(node.range.start);
    let lines = code.is_empty().then(Vec::new).unwrap_or_else(|| {
        code.split('\n')
            .enumerate()
            .map(|(index, line)| {
                let line_offset = code_line_offset(&content_bytes, &code, index);
                let source_line = if fenced {
                    line_index.line_for_offset(line_offset)
                } else {
                    first_source_line + index
                };
                let line_source = if fenced {
                    let end = line_offset + line.len();
                    source_range(line_offset..end, line_index)
                } else {
                    line_index.source_line_range(source_line)
                };
                MarkdownCodeLine {
                    index,
                    source: line_source,
                    source_line: Some(source_line),
                    text: line.strip_suffix('\r').unwrap_or(line).to_owned(),
                    target_id: None,
                }
            })
            .collect()
    });
    MarkdownCodeBlock {
        language,
        info,
        source: source_range_value,
        content: source_range(content_bytes, line_index),
        lines,
        target_id: None,
    }
}

fn code_line_offset(content: &Range<usize>, code: &str, index: usize) -> usize {
    content.start
        + code
            .split_inclusive('\n')
            .take(index)
            .map(str::len)
            .sum::<usize>()
}

fn code_content_range(
    block: Range<usize>,
    kind: &CodeBlockKind<'static>,
    source: &str,
) -> Range<usize> {
    if !matches!(kind, CodeBlockKind::Fenced(_)) {
        return block;
    }
    let start = block.start.min(source.len());
    let end = block.end.min(source.len());
    let opening_end = source[start..end]
        .find('\n')
        .map_or(end, |offset| start + offset + 1);
    if opening_end >= end {
        return opening_end..opening_end;
    }
    let opening_line = source[start..opening_end].trim_end_matches(['\r', '\n']);
    let trimmed = opening_line.trim_start();
    let marker = trimmed.chars().next().unwrap_or('`');
    let marker_len = trimmed
        .chars()
        .take_while(|character| *character == marker)
        .count();
    let mut closing_start = None;
    let mut cursor = opening_end;
    while cursor < end {
        let line_end = source[cursor..end]
            .find('\n')
            .map_or(end, |offset| cursor + offset + 1);
        let line = source[cursor..line_end].trim_end_matches(['\r', '\n']);
        let candidate = line.trim_start();
        let count = candidate
            .chars()
            .take_while(|character| *character == marker)
            .count();
        if count >= marker_len && marker_len > 0 && candidate[count..].trim().is_empty() {
            closing_start = Some(cursor);
            break;
        }
        cursor = line_end;
    }
    let mut content_end = closing_start.unwrap_or(end);
    if closing_start.is_some() && source[..content_end].ends_with('\n') {
        content_end -= 1;
        if content_end > opening_end && source.as_bytes()[content_end - 1] == b'\r' {
            content_end -= 1;
        }
    }
    opening_end.min(content_end)..content_end
}

fn parse_table(node: &Node, alignments: &[Alignment], line_index: &LineIndex) -> MarkdownTable {
    let rows = node
        .children
        .iter()
        .filter_map(|section| {
            let header = matches!(section.event, Some(Event::Start(Tag::TableHead)));
            if !matches!(section.event, Some(Event::Start(Tag::TableHead)))
                && !matches!(section.event, Some(Event::Start(Tag::TableRow)))
            {
                return None;
            }
            table_row(section, header, line_index)
        })
        .collect();
    MarkdownTable {
        alignments: alignments.iter().copied().map(table_alignment).collect(),
        rows,
    }
}

fn table_row(node: &Node, header: bool, line_index: &LineIndex) -> Option<MarkdownTableRow> {
    if !matches!(
        node.event,
        Some(Event::Start(Tag::TableRow | Tag::TableHead))
    ) {
        return None;
    }
    let cells = node
        .children
        .iter()
        .filter_map(|cell| {
            if !matches!(cell.event, Some(Event::Start(Tag::TableCell))) {
                return None;
            }
            Some(MarkdownTableCell {
                source: source_range(cell.range.clone(), line_index),
                content: inline_children(&cell.children),
            })
        })
        .collect();
    Some(MarkdownTableRow {
        header,
        source: source_range(node.range.clone(), line_index),
        cells,
        target_id: None,
    })
}

fn table_alignment(alignment: Alignment) -> MarkdownTableAlignment {
    match alignment {
        Alignment::None => MarkdownTableAlignment::None,
        Alignment::Left => MarkdownTableAlignment::Left,
        Alignment::Center => MarkdownTableAlignment::Center,
        Alignment::Right => MarkdownTableAlignment::Right,
    }
}

fn inline_children(children: &[Node]) -> Vec<MarkdownInline> {
    children.iter().filter_map(inline_node).collect()
}

fn inline_node(node: &Node) -> Option<MarkdownInline> {
    let event = node.event.as_ref()?;
    Some(match event {
        Event::Text(text) => MarkdownInline::Text(text.to_string()),
        Event::Code(text) => MarkdownInline::Code(text.to_string()),
        Event::Start(Tag::Strong) => MarkdownInline::Strong(inline_children(&node.children)),
        Event::Start(Tag::Emphasis) => MarkdownInline::Emphasis(inline_children(&node.children)),
        Event::Start(Tag::Strikethrough) => {
            MarkdownInline::Strikethrough(inline_children(&node.children))
        }
        Event::Start(Tag::Link {
            dest_url, title, ..
        }) => MarkdownInline::Link {
            destination: dest_url.to_string(),
            title: (!title.is_empty()).then(|| title.to_string()),
            content: inline_children(&node.children),
        },
        Event::Start(Tag::Image { .. }) => {
            MarkdownInline::ImageAlt(rendered_text(&inline_children(&node.children)))
        }
        Event::SoftBreak => MarkdownInline::SoftBreak,
        Event::HardBreak => MarkdownInline::HardBreak,
        Event::InlineHtml(html) | Event::Html(html) => MarkdownInline::Text(html.to_string()),
        Event::TaskListMarker(checked) => MarkdownInline::Text(if *checked {
            "☑".to_owned()
        } else {
            "☐".to_owned()
        }),
        // Math, footnotes, and metadata are not enabled by this parser. Keep a
        // useful textual fallback if a future option starts producing them.
        Event::InlineMath(text) | Event::DisplayMath(text) | Event::FootnoteReference(text) => {
            MarkdownInline::Text(text.to_string())
        }
        Event::Start(_) | Event::End(_) | Event::Rule => return None,
    })
}

fn html_fallback(children: &[Node]) -> Vec<MarkdownInline> {
    children
        .iter()
        .filter_map(|child| match child.event.as_ref()? {
            Event::Html(text) | Event::InlineHtml(text) => {
                Some(MarkdownInline::Text(text.to_string()))
            }
            _ => None,
        })
        .collect()
}

fn assign_targets(
    blocks: &mut [MarkdownBlock],
    targets: &mut Vec<MarkdownTarget>,
    outline: &mut Vec<MarkdownHeading>,
) {
    for block in blocks {
        let mut block_target = None;
        match &mut block.kind {
            MarkdownBlockKind::Heading { level, content } => {
                let title = rendered_text(content);
                let id = push_target(
                    MarkdownTargetKind::Heading,
                    block.source.clone(),
                    title.clone(),
                    targets,
                );
                block_target = Some(id);
                outline.push(MarkdownHeading {
                    level: *level,
                    title,
                    source: block.source.clone(),
                    target_id: id,
                });
            }
            MarkdownBlockKind::Paragraph { content } => {
                block_target = Some(push_target(
                    MarkdownTargetKind::Paragraph,
                    block.source.clone(),
                    rendered_text(content),
                    targets,
                ));
            }
            MarkdownBlockKind::List { items, .. } => {
                for item in items {
                    let id = MarkdownTargetId(targets.len());
                    item.target_id = Some(id);
                    targets.push(MarkdownTarget {
                        id,
                        kind: MarkdownTargetKind::ListItem,
                        source: item.source.clone(),
                        display_label: rendered_text(&item.content),
                    });
                    assign_nested_list_targets(&mut item.blocks, targets);
                }
            }
            MarkdownBlockKind::BlockQuote { blocks: children } => {
                block_target = Some(push_target(
                    MarkdownTargetKind::BlockQuote,
                    block.source.clone(),
                    block_text(children),
                    targets,
                ));
            }
            MarkdownBlockKind::CodeBlock(code) => {
                let id = MarkdownTargetId(targets.len());
                code.target_id = Some(id);
                targets.push(MarkdownTarget {
                    id,
                    kind: MarkdownTargetKind::CodeBlock,
                    source: code.source.clone(),
                    display_label: "Code block".to_owned(),
                });
                block_target = Some(id);
                for line in &mut code.lines {
                    let id = MarkdownTargetId(targets.len());
                    line.target_id = Some(id);
                    targets.push(MarkdownTarget {
                        id,
                        kind: MarkdownTargetKind::CodeLine,
                        source: line.source.clone(),
                        display_label: format!("Code line {}", line.index + 1),
                    });
                }
            }
            MarkdownBlockKind::Table(table) => {
                for row in &mut table.rows {
                    let id = MarkdownTargetId(targets.len());
                    row.target_id = Some(id);
                    targets.push(MarkdownTarget {
                        id,
                        kind: MarkdownTargetKind::TableRow,
                        source: row.source.clone(),
                        display_label: row_text(row),
                    });
                }
            }
            MarkdownBlockKind::HtmlFallback { .. } | MarkdownBlockKind::Rule => {}
        }
        block.target_id = block_target;
    }
}

fn assign_nested_list_targets(blocks: &mut [MarkdownBlock], targets: &mut Vec<MarkdownTarget>) {
    for block in blocks {
        if let MarkdownBlockKind::List { items, .. } = &mut block.kind {
            for item in items {
                let id = MarkdownTargetId(targets.len());
                item.target_id = Some(id);
                targets.push(MarkdownTarget {
                    id,
                    kind: MarkdownTargetKind::ListItem,
                    source: item.source.clone(),
                    display_label: rendered_text(&item.content),
                });
                assign_nested_list_targets(&mut item.blocks, targets);
            }
        }
    }
}

fn push_target(
    kind: MarkdownTargetKind,
    source: SourceRange,
    display_label: String,
    targets: &mut Vec<MarkdownTarget>,
) -> MarkdownTargetId {
    let id = MarkdownTargetId(targets.len());
    targets.push(MarkdownTarget {
        id,
        kind,
        source,
        display_label,
    });
    id
}

fn block_text(blocks: &[MarkdownBlock]) -> String {
    blocks
        .iter()
        .map(|block| match &block.kind {
            MarkdownBlockKind::Heading { content, .. }
            | MarkdownBlockKind::Paragraph { content }
            | MarkdownBlockKind::HtmlFallback { content } => rendered_text(content),
            MarkdownBlockKind::List { items, .. } => items
                .iter()
                .map(|item| rendered_text(&item.content))
                .collect::<Vec<_>>()
                .join(" "),
            MarkdownBlockKind::BlockQuote { blocks } => block_text(blocks),
            MarkdownBlockKind::CodeBlock(code) => code
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            MarkdownBlockKind::Table(table) => table
                .rows
                .iter()
                .map(row_text)
                .collect::<Vec<_>>()
                .join(" "),
            MarkdownBlockKind::Rule => String::new(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn row_text(row: &MarkdownTableRow) -> String {
    row.cells
        .iter()
        .map(|cell| rendered_text(&cell.content))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[derive(Debug, Clone)]
struct LineIndex {
    starts: Vec<usize>,
    content_ends: Vec<usize>,
    source_len: usize,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        let mut content_ends = Vec::new();
        for (index, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                content_ends.push(index.saturating_sub(usize::from(
                    index > 0 && source.as_bytes()[index - 1] == b'\r',
                )));
                starts.push(index + 1);
            }
        }
        content_ends.push(source.len());
        Self {
            starts,
            content_ends,
            source_len: source.len(),
        }
    }

    fn line_for_offset(&self, offset: usize) -> usize {
        let offset = offset.min(*self.starts.last().unwrap_or(&0));
        self.starts.partition_point(|start| *start <= offset)
    }

    fn range(&self, range: Range<usize>) -> MarkdownLineRange {
        let start = self.line_for_offset(range.start);
        let end_offset = range.end.saturating_sub(1).max(range.start);
        let end = self.line_for_offset(end_offset).max(start);
        MarkdownLineRange { start, end }
    }

    fn source_line_range(&self, line: usize) -> SourceRange {
        let start = self
            .starts
            .get(line.saturating_sub(1))
            .copied()
            .unwrap_or(0);
        let end = self
            .content_ends
            .get(line.saturating_sub(1))
            .copied()
            .unwrap_or(self.source_len)
            .max(start);
        SourceRange {
            bytes: start..end,
            lines: MarkdownLineRange {
                start: line,
                end: line,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_targets_in_document_order() {
        let document = MarkdownDocument::parse("# Title\n\nText\n\n```rust\nlet x = 1;\n```\n");
        assert_eq!(document.targets().len(), 4);
        assert_eq!(document.targets()[0].kind, MarkdownTargetKind::Heading);
        assert_eq!(document.targets()[1].kind, MarkdownTargetKind::Paragraph);
        assert_eq!(document.targets()[2].kind, MarkdownTargetKind::CodeBlock);
        assert_eq!(document.targets()[3].kind, MarkdownTargetKind::CodeLine);
        assert_eq!(document.outline()[0].title, "Title");
    }

    #[test]
    fn preserves_source_and_maps_crlf_lines() {
        let source = "# Héading\r\n\r\n```\r\none\r\ntwo\r\n```\r\n";
        let document = MarkdownDocument::parse(source);
        assert_eq!(document.source(), source);
        let MarkdownBlockKind::CodeBlock(code) = &document.blocks()[1].kind else {
            panic!("expected code block")
        };
        assert_eq!(
            code.lines
                .iter()
                .map(|line| line.source_line)
                .collect::<Vec<_>>(),
            vec![Some(4), Some(5)]
        );
        assert_eq!(code.lines[0].text, "one");
    }

    #[test]
    fn parses_nested_inline_nodes_and_table_rows() {
        let document = MarkdownDocument::parse(
            "**bold _em_** and ~~gone~~ [link](url) ![alt](image)\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n",
        );
        assert!(matches!(
            document.blocks()[0].kind,
            MarkdownBlockKind::Paragraph { .. }
        ));
        let MarkdownBlockKind::Table(table) = &document.blocks()[1].kind else {
            panic!("expected table")
        };
        assert_eq!(table.rows.len(), 2);
        assert_eq!(
            document
                .targets()
                .iter()
                .filter(|target| target.kind == MarkdownTargetKind::TableRow)
                .count(),
            2
        );
    }

    #[test]
    fn longer_outer_fence_does_not_close_on_shorter_run() {
        let document = MarkdownDocument::parse("````\n```\nstill code\n````\n");
        let MarkdownBlockKind::CodeBlock(code) = &document.blocks()[0].kind else {
            panic!("expected code block")
        };
        assert_eq!(code.lines.len(), 2);
        assert_eq!(code.lines[0].text, "```");
    }

    #[test]
    fn blockquote_is_the_only_target_for_its_contents() {
        let document = MarkdownDocument::parse("> # quoted\n>\n> text\n");
        assert_eq!(document.targets().len(), 1);
        assert_eq!(document.targets()[0].kind, MarkdownTargetKind::BlockQuote);
    }

    #[test]
    fn nested_list_items_are_targets_without_overlapping_paragraph_targets() {
        let document = MarkdownDocument::parse("1. outer\n   - inner\n   - second\n2. last\n");
        let kinds = document
            .targets()
            .iter()
            .map(|target| target.kind)
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                MarkdownTargetKind::ListItem,
                MarkdownTargetKind::ListItem,
                MarkdownTargetKind::ListItem,
                MarkdownTargetKind::ListItem,
            ]
        );
        let MarkdownBlockKind::List { items, .. } = &document.blocks()[0].kind else {
            panic!("expected list")
        };
        assert_eq!(items[0].depth, 0);
        assert_eq!(items[0].blocks.len(), 1);
    }

    #[test]
    fn indented_code_strips_parser_indentation() {
        let document = MarkdownDocument::parse("    first\n    second\n");
        let MarkdownBlockKind::CodeBlock(code) = &document.blocks()[0].kind else {
            panic!("expected code block")
        };
        assert_eq!(
            code.lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert_eq!(code.lines[0].source_line, Some(1));
        assert_eq!(code.lines[1].source.bytes, 10..20);
    }

    #[test]
    fn indented_code_preserves_blank_content_lines() {
        let document = MarkdownDocument::parse("    one\n\n    two\n");
        let MarkdownBlockKind::CodeBlock(code) = &document.blocks()[0].kind else {
            panic!("expected code block")
        };
        assert_eq!(
            code.lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["one", "", "two"]
        );
        assert_eq!(
            code.lines
                .iter()
                .map(|line| line.source_line)
                .collect::<Vec<_>>(),
            [Some(1), Some(2), Some(3)]
        );
    }

    #[test]
    fn raw_html_is_plain_fallback_without_a_target() {
        let document = MarkdownDocument::parse("<div>not executed</div>\n");
        assert!(matches!(
            document.blocks()[0].kind,
            MarkdownBlockKind::HtmlFallback { .. }
        ));
        assert!(document.targets().is_empty());
    }

    #[test]
    fn empty_and_malformed_documents_are_safe() {
        assert!(MarkdownDocument::parse("").targets().is_empty());
        let document = MarkdownDocument::parse("# unclosed **emphasis\n\n```rust\ncode");
        assert!(!document.blocks().is_empty());
    }
}
