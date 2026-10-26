use crate::document::{
    LineIndex, MarkdownBlock, MarkdownBlockKind, MarkdownCodeBlock, MarkdownDocument,
    MarkdownSourceStyle, MarkdownTarget, MarkdownTargetId, MarkdownTargetKind, assign_targets,
    collect_source_styles, event_tree, fence_marker, fenced_code_line, fenced_code_lines,
    fenced_content_bounds, parse_block, parser_options, source_range,
};
use pulldown_cmark::Parser;
use std::{borrow::Cow, collections::BTreeMap, ops::Range};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarkdownParseStats {
    pub parsed_bytes: usize,
    pub scanned_bytes: usize,
    pub source_bytes_copied: usize,
    pub prefix_bytes_copied: usize,
    pub parses: u64,
    pub unsettled_blocks: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Definition {
    destination: String,
    title: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct Settled {
    blocks: usize,
    targets: usize,
    outline: usize,
    styles: usize,
    source_end: usize,
    brackets: Vec<bool>,
    definitions: Vec<Range<usize>>,
    prefix: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FenceLine {
    Indent(usize),
    Marker(usize),
    Trailing,
    Content,
}

impl FenceLine {
    fn feed(self, byte: u8, marker: u8, count: usize) -> Self {
        match self {
            Self::Indent(columns) => match byte {
                b' ' if columns < 3 => Self::Indent(columns + 1),
                byte if byte == marker => Self::Marker(1),
                _ => Self::Content,
            },
            Self::Marker(run) => {
                if byte == marker {
                    Self::Marker(run + 1)
                } else if byte == b' ' && run >= count {
                    Self::Trailing
                } else {
                    Self::Content
                }
            }
            Self::Trailing => {
                if byte == b' ' {
                    Self::Trailing
                } else {
                    Self::Content
                }
            }
            Self::Content => Self::Content,
        }
    }

    fn closes(self, count: usize) -> bool {
        match self {
            Self::Marker(run) => run >= count,
            Self::Trailing => true,
            Self::Indent(_) | Self::Content => false,
        }
    }
}

#[derive(Debug, Clone)]
struct OpenFence {
    block: usize,
    marker: u8,
    count: usize,
    start: usize,
    content_start: usize,
    line_start: usize,
    complete: usize,
    state: FenceLine,
}

struct TailParse {
    blocks: Vec<MarkdownBlock>,
    styles: Vec<MarkdownSourceStyle>,
    definitions: BTreeMap<String, Definition>,
    tail_definitions: Vec<Range<usize>>,
    parsed_bytes: usize,
    copied_bytes: usize,
    prefix_bytes: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct IncrementalDocument {
    document: MarkdownDocument,
    index: LineIndex,
    settled: Settled,
    definitions: BTreeMap<String, Definition>,
    tail_definitions: Vec<Range<usize>>,
    fence: Option<OpenFence>,
    stats: MarkdownParseStats,
}

impl IncrementalDocument {
    pub(crate) const fn document(&self) -> &MarkdownDocument {
        &self.document
    }

    pub(crate) const fn stats(&self) -> MarkdownParseStats {
        self.stats
    }

    pub(crate) const fn settled_blocks(&self) -> usize {
        self.settled.blocks
    }

    pub(crate) fn open_code_block(&self) -> Option<usize> {
        self.fence.as_ref().map(|fence| fence.block)
    }

    pub(crate) fn replace(&mut self, source: &str) -> usize {
        let stats = self.stats;
        *self = Self {
            stats,
            ..Self::default()
        };
        self.append(source);
        0
    }

    pub(crate) fn append(&mut self, chunk: &str) -> usize {
        let start = self.document.source.len();
        self.document.source.push_str(chunk);
        self.stats.source_bytes_copied += chunk.len();
        self.stats.scanned_bytes += chunk.len();
        self.index.extend(&self.document.source, start);
        if self.fence.is_some() {
            let block = self.fence.as_ref().map_or(0, |fence| fence.block);
            if let Some(resume) = self.extend_fence(start) {
                self.settle_to(block + 1, resume);
                self.fence = None;
                if resume < self.document.source.len() {
                    self.parse_tail();
                }
            }
            return block;
        }
        self.parse_tail()
    }

    fn parse_tail(&mut self) -> usize {
        let mut first_changed = self.settled.blocks;
        loop {
            first_changed = first_changed.min(self.settled.blocks);
            let parsed = parse_from(
                &self.document.source,
                &self.index,
                self.settled.source_end,
                &self.settled.prefix,
            );
            self.stats.parsed_bytes += parsed.parsed_bytes;
            self.stats.source_bytes_copied += parsed.copied_bytes;
            self.stats.prefix_bytes_copied += parsed.prefix_bytes;
            self.stats.parses += 1;
            self.install(parsed.blocks, parsed.styles);
            if parsed.definitions != self.definitions {
                self.definitions = parsed.definitions;
                if let Some(block) = self.settled.brackets.iter().position(|bracket| *bracket) {
                    self.unsettle(block);
                    self.stats.unsettled_blocks += 1;
                    continue;
                }
            }
            self.tail_definitions = parsed.tail_definitions;
            break;
        }
        self.settle_tail();
        self.detect_fence();
        first_changed
    }

    fn install(&mut self, blocks: Vec<MarkdownBlock>, styles: Vec<MarkdownSourceStyle>) {
        let settled = &self.settled;
        let document = &mut self.document;
        document.blocks.truncate(settled.blocks);
        document.blocks.extend(blocks);
        document.source_styles.truncate(settled.styles);
        document.source_styles.extend(styles);
        document.targets.truncate(settled.targets);
        document.outline.truncate(settled.outline);
        assign_targets(
            &mut document.blocks[settled.blocks..],
            &mut document.targets,
            &mut document.outline,
        );
    }

    fn unsettle(&mut self, block: usize) {
        let blocks = &self.document.blocks;
        let gap_start = block
            .checked_sub(1)
            .map_or(0, |previous| blocks[previous].source.bytes.end);
        let end = self.attached_start(blocks[block].source.bytes.start, gap_start);
        self.settled.blocks = block;
        self.settled.source_end = end;
        self.settled.brackets.truncate(block);
        self.settled.definitions.retain(|span| span.end <= end);
        self.settled.prefix.clear();
        for span in &self.settled.definitions {
            self.stats.prefix_bytes_copied += span.len() + 1;
            self.stats.source_bytes_copied += span.len() + 1;
            self.settled
                .prefix
                .push_str(&self.document.source[span.clone()]);
            self.settled.prefix.push('\n');
        }
        self.sync_settled_counts();
        self.fence = None;
    }

    fn sync_settled_counts(&mut self) {
        let end = self.settled.source_end;
        let document = &self.document;
        self.settled.targets = document
            .targets
            .partition_point(|target| target.source.bytes.start < end);
        self.settled.outline = document
            .outline
            .partition_point(|heading| heading.source.bytes.start < end);
        self.settled.styles = document
            .source_styles
            .partition_point(|style| style.source.bytes.start < end);
    }

    fn settle_tail(&mut self) {
        let blocks = &self.document.blocks;
        let mut settled = self.settled.blocks;
        let mut end = self.settled.source_end;
        while settled < blocks.len() {
            let Some(boundary) = self.settle_boundary(&blocks[settled], blocks.get(settled + 1))
            else {
                break;
            };
            end = boundary;
            settled += 1;
        }
        if settled > self.settled.blocks {
            self.settle_to(settled, end);
        }
    }

    fn settle_to(&mut self, blocks: usize, end: usize) {
        let source = &self.document.source;
        for block in &self.document.blocks[self.settled.blocks..blocks] {
            let bracket = !matches!(block.kind, MarkdownBlockKind::CodeBlock(_)) && {
                let bytes = &source[block.source.bytes.clone()];
                self.stats.scanned_bytes += bytes.len();
                bytes.contains('[')
            };
            self.settled.brackets.push(bracket);
        }
        self.settled.blocks = blocks;
        self.settled.source_end = end;
        self.sync_settled_counts();
        let mut remaining = Vec::new();
        for span in std::mem::take(&mut self.tail_definitions) {
            if span.end <= end {
                self.stats.prefix_bytes_copied += span.len() + 1;
                self.stats.source_bytes_copied += span.len() + 1;
                self.settled
                    .prefix
                    .push_str(&self.document.source[span.clone()]);
                self.settled.prefix.push('\n');
                self.settled.definitions.push(span);
            } else {
                remaining.push(span);
            }
        }
        self.tail_definitions = remaining;
    }

    fn settle_boundary(
        &self,
        block: &MarkdownBlock,
        next: Option<&MarkdownBlock>,
    ) -> Option<usize> {
        let source = self.document.source.as_str();
        if let Some(next) = next {
            let start = next.source.bytes.start;
            return self
                .index
                .line_complete(start)
                .then(|| self.attached_start(start, block.source.bytes.end))
                .filter(|boundary| *boundary >= block.source.bytes.end);
        }
        let end = block.source.bytes.end;
        match &block.kind {
            MarkdownBlockKind::Heading { .. } | MarkdownBlockKind::Rule => {
                source[..end].ends_with('\n').then_some(end)
            }
            MarkdownBlockKind::Paragraph { .. }
            | MarkdownBlockKind::Table(_)
            | MarkdownBlockKind::BlockQuote { .. } => blank_line_at(source, end).then_some(end),
            MarkdownBlockKind::CodeBlock(code) => {
                (code.info.is_some() && end < source.len()).then(|| after_eol(source, end))
            }
            MarkdownBlockKind::List { .. } | MarkdownBlockKind::HtmlFallback { .. } => None,
        }
    }

    fn attached_start(&self, start: usize, gap_start: usize) -> usize {
        let source = self.document.source.as_str();
        let mut start = self.index.line_start_of(start);
        while start > gap_start {
            let previous = self.index.line_start_of(start - 1);
            if previous < gap_start || blank_line_at(source, previous) {
                break;
            }
            start = previous;
        }
        start
    }

    fn detect_fence(&mut self) {
        let source = self.document.source.as_str();
        let len = source.len();
        let index = self.document.blocks.len().checked_sub(1);
        let Some(index) = index.filter(|index| *index >= self.settled.blocks) else {
            return;
        };
        let block = &self.document.blocks[index];
        let MarkdownBlockKind::CodeBlock(code) = &block.kind else {
            return;
        };
        let start = block.source.bytes.start;
        if code.info.is_none() || block.source.bytes.end != len || !self.index.line_complete(start)
        {
            return;
        }
        let opening_end = source[start..]
            .find('\n')
            .map_or(len, |offset| start + offset + 1);
        let (marker, count) = fence_marker(&source[start..opening_end]);
        let line_start = self.index.line_start_of(len).max(opening_end);
        let mut state = FenceLine::Indent(0);
        for byte in source[line_start..].bytes() {
            state = state.feed(byte, marker, count);
        }
        self.stats.scanned_bytes += len - line_start;
        self.fence = Some(OpenFence {
            block: index,
            marker,
            count,
            start,
            content_start: opening_end,
            line_start,
            complete: self.index.line_breaks_after(opening_end),
            state,
        });
    }

    fn extend_fence(&mut self, start: usize) -> Option<usize> {
        let mut fence = self.fence.take()?;
        let MarkdownDocument {
            source,
            blocks,
            targets,
            ..
        } = &mut self.document;
        let bytes = source.as_bytes();
        let len = bytes.len();
        let MarkdownBlockKind::CodeBlock(code) = &mut blocks[fence.block].kind else {
            return None;
        };
        let previous_lines = code.lines.len();
        code.lines.truncate(fence.complete);
        let mut close = None;
        let mut position = start;
        while position < len {
            let byte = bytes[position];
            match byte {
                b'\n' | b'\r' if fence.state.closes(fence.count) => {
                    let resume = position
                        + 1
                        + usize::from(byte == b'\r' && bytes.get(position + 1) == Some(&b'\n'));
                    close = Some((position, resume));
                    break;
                }
                b'\n' => {
                    code.lines.push(fenced_code_line(
                        fence.complete,
                        fence.line_start,
                        &source[fence.line_start..position],
                        &self.index,
                    ));
                    fence.complete += 1;
                    fence.line_start = position + 1;
                    fence.state = FenceLine::Indent(0);
                }
                _ => fence.state = fence.state.feed(byte, fence.marker, fence.count),
            }
            position += 1;
        }
        self.stats.scanned_bytes += len - start;
        let block_end = close.map_or(len, |(end, _)| end);
        let closing_start = (close.is_some() || fence.state.closes(fence.count))
            .then_some(fence.line_start.max(fence.content_start));
        let content = fenced_content_bounds(fence.content_start, closing_start, block_end, source);
        let first_changed_line = if closing_start.is_some() || code.lines.len() < fence.complete {
            self.stats.scanned_bytes += content.len();
            code.lines = fenced_code_lines(content.clone(), source, &self.index);
            0
        } else {
            if let Some(last) = code.lines.last_mut() {
                last.source =
                    source_range(last.source.bytes.start..fence.line_start - 1, &self.index);
            }
            if len > fence.content_start {
                self.stats.scanned_bytes += len - fence.line_start;
                code.lines.push(fenced_code_line(
                    fence.complete,
                    fence.line_start,
                    &source[fence.line_start..],
                    &self.index,
                ));
            }
            previous_lines.saturating_sub(1).min(fence.complete)
        };
        code.content = source_range(content, &self.index);
        code.source = source_range(fence.start..block_end, &self.index);
        let block_source = code.source.clone();
        sync_code_targets(code, targets, first_changed_line);
        blocks[fence.block].source = block_source;
        let resume = close.map(|(_, resume)| resume);
        if resume.is_none() {
            self.fence = Some(fence);
        }
        resume
    }
}

fn sync_code_targets(code: &mut MarkdownCodeBlock, targets: &mut Vec<MarkdownTarget>, from: usize) {
    let Some(base) = code.target_id.map(MarkdownTargetId::index) else {
        return;
    };
    if let Some(target) = targets.get_mut(base) {
        target.source = code.source.clone();
    }
    targets.truncate(base + 1 + from);
    for line in &mut code.lines[from..] {
        let id = MarkdownTargetId::new(targets.len());
        line.target_id = Some(id);
        targets.push(MarkdownTarget {
            id,
            kind: MarkdownTargetKind::CodeLine,
            source: line.source.clone(),
            display_label: format!("Code line {}", line.index + 1),
        });
    }
}

fn parse_from(source: &str, index: &LineIndex, tail_start: usize, prefix: &str) -> TailParse {
    let tail = &source[tail_start..];
    let (text, shift) = if prefix.is_empty() {
        (Cow::Borrowed(tail), 0)
    } else {
        (Cow::Owned(format!("{prefix}\n{tail}")), prefix.len() + 1)
    };
    let parser = Parser::new_ext(&text, parser_options());
    let mut definitions = BTreeMap::new();
    let mut tail_definitions = Vec::new();
    for (label, definition) in parser.reference_definitions().iter() {
        definitions.insert(
            label.to_owned(),
            Definition {
                destination: definition.dest.to_string(),
                title: definition.title.as_ref().map(ToString::to_string),
            },
        );
        if definition.span.start >= shift {
            tail_definitions.push(
                definition.span.start - shift + tail_start
                    ..definition.span.end - shift + tail_start,
            );
        }
    }
    tail_definitions.sort_by_key(|span| span.start);
    let events = parser
        .into_offset_iter()
        .filter(|(_, range)| range.start >= shift)
        .map(|(event, range)| {
            (
                event.into_static(),
                range.start - shift + tail_start..range.end - shift + tail_start,
            )
        })
        .collect::<Vec<_>>();
    let roots = event_tree(&events);
    let mut styles = Vec::new();
    collect_source_styles(&roots, index, &mut styles);
    let blocks = roots
        .iter()
        .filter_map(|node| parse_block(node, 0, source, index))
        .collect();
    TailParse {
        blocks,
        styles,
        definitions,
        tail_definitions,
        parsed_bytes: text.len(),
        copied_bytes: if matches!(text, Cow::Owned(_)) {
            text.len()
        } else {
            0
        },
        prefix_bytes: if prefix.is_empty() {
            0
        } else {
            prefix.len() + 1
        },
    }
}

fn blank_line_at(source: &str, position: usize) -> bool {
    let mut cursor = position;
    let bytes = source.as_bytes();
    while let Some(byte) = bytes.get(cursor) {
        match byte {
            b' ' | b'\t' | 0x0b | 0x0c => cursor += 1,
            b'\n' | b'\r' => return true,
            _ => return false,
        }
    }
    false
}

fn after_eol(source: &str, position: usize) -> usize {
    let bytes = source.as_bytes();
    match (bytes.get(position), bytes.get(position + 1)) {
        (Some(b'\r'), Some(b'\n')) => position + 2,
        (Some(b'\n' | b'\r'), _) => position + 1,
        _ => position,
    }
}
