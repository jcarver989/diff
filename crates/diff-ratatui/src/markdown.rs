//! Read-only whole-document and append-stream Markdown rendering.

use crate::{
    color::{layered_style, page_color},
    markdown_layout::{
        MarkdownLayout, MarkdownLayoutOptions, MarkdownPresentation, MarkdownRow,
        MarkdownRowUpdate, RowChunk, RowStore,
    },
    syntax::highlighted_line,
    text::{FitOptions, fit_spans},
};
use clankerdiff_markdown::{
    MarkdownBlock, MarkdownBlockKind, MarkdownCodeBlock, MarkdownDocument, MarkdownInline,
    MarkdownLineRange, MarkdownListItem, MarkdownSourceRole, MarkdownSourceStyle, MarkdownStream,
    MarkdownStreamIdentity, MarkdownTable, MarkdownTableAlignment, MarkdownTargetId, SourceRange,
    rendered_text,
};
use clankerdiff_syntax::{
    DocumentHighlights, HighlightSpan, LanguageHint, SourceSequenceId, SyntaxHighlighter,
};
use clankerdiff_theme::{Fingerprint, ReviewTheme, Rgba};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use std::{collections::HashMap, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Deterministic work counters for incremental Markdown rendering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarkdownRenderStats {
    /// Bytes actually supplied to the Markdown parser.
    pub parsed_bytes: usize,
    pub parsed_documents: u64,
    pub rows_generated: usize,
    pub rows_reused: usize,
    pub rows_materialized: usize,
    pub highlighted_bytes: usize,
}

/// Renderer-owned cache for one logical streaming Markdown item.
#[derive(Debug, Clone, Default)]
pub struct StreamingMarkdownState {
    layout: MarkdownLayout,
    cache: LayoutCache,
    revision: u64,
    /// Last revision handed out; survives `reset` so hosts never see a reuse.
    next_revision: u64,
    update: Option<MarkdownRowUpdate>,
    stats: MarkdownRenderStats,
}

impl StreamingMarkdownState {
    pub fn reset(&mut self) {
        *self = Self {
            next_revision: self.next_revision,
            ..Self::default()
        };
    }

    /// Returns accumulated renderer work counters and resets them.
    pub fn take_stats(&mut self) -> MarkdownRenderStats {
        std::mem::take(&mut self.stats)
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn update_since(&self, base_revision: u64) -> MarkdownRowUpdate {
        let rows = self.layout.rows();
        if base_revision == self.revision {
            return MarkdownRowUpdate {
                base_revision,
                revision: self.revision,
                first_changed_row: rows.len(),
                replacement: rows.slice(rows.len()..rows.len()),
                reset: false,
            };
        }
        if let Some(update) = &self.update
            && update.base_revision == base_revision
        {
            return update.clone();
        }
        MarkdownRowUpdate {
            base_revision,
            revision: self.revision,
            first_changed_row: 0,
            replacement: rows.clone(),
            reset: true,
        }
    }
}

#[derive(Debug, Clone)]
struct ParsedSource {
    identity: MarkdownStreamIdentity,
    revision: u64,
    document: Arc<MarkdownDocument>,
}

impl ParsedSource {
    fn parse(stream: &MarkdownStream) -> Self {
        Self {
            identity: stream.identity(),
            revision: stream.revision(),
            document: Arc::new(MarkdownDocument::parse(stream.source())),
        }
    }

    fn matches(&self, stream: &MarkdownStream) -> bool {
        self.identity == stream.identity() && self.revision == stream.revision()
    }
}

/// Stateless whole-document renderer plus streaming cache services.
#[derive(Debug, Default)]
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Renders a canonical semantic document without review gutters or controls.
    #[must_use]
    pub fn render_lines(
        &self,
        document: &MarkdownDocument,
        options: MarkdownLayoutOptions,
        theme: &ReviewTheme,
        highlighter: &mut SyntaxHighlighter,
    ) -> Arc<[Line<'static>]> {
        self.render_layout(document, options, theme, highlighter)
            .materialize()
    }

    pub fn render_stream_lines(
        &self,
        state: &mut StreamingMarkdownState,
        stream: &MarkdownStream,
        options: MarkdownLayoutOptions,
        theme: &ReviewTheme,
        highlighter: &mut SyntaxHighlighter,
    ) -> Arc<[Line<'static>]> {
        let layout = self.render_stream_layout(state, stream, options, theme, highlighter);
        if !layout.is_materialized() {
            state.stats.rows_materialized += layout.row_count();
        }
        layout.materialize()
    }

    #[must_use]
    pub fn render_layout(
        &self,
        document: &MarkdownDocument,
        options: MarkdownLayoutOptions,
        theme: &ReviewTheme,
        highlighter: &mut SyntaxHighlighter,
    ) -> MarkdownLayout {
        layout_document(document, options, theme, highlighter, None)
            .0
            .finish(document)
            .layout
    }

    pub fn render_stream_layout(
        &self,
        state: &mut StreamingMarkdownState,
        stream: &MarkdownStream,
        options: MarkdownLayoutOptions,
        theme: &ReviewTheme,
        highlighter: &mut SyntaxHighlighter,
    ) -> MarkdownLayout {
        let revision = theme.revision();
        if state.cache.is_current(stream, options, revision) {
            state.stats.rows_reused += state.layout.row_count();
            return state.layout.clone();
        }
        let reset = state.cache.resets_for(stream, options, revision);
        let highlighted_before = highlighter.stats().bytes;
        let built = state.cache.render(
            stream,
            options,
            theme,
            revision,
            highlighter,
            &mut state.stats,
        );
        state.stats.highlighted_bytes +=
            highlighter.stats().bytes.saturating_sub(highlighted_before);
        let first_changed_row = if reset {
            0
        } else {
            state
                .layout
                .rows()
                .iter()
                .zip(built.layout.rows().iter())
                .take_while(|(left, right)| Arc::ptr_eq(left, right) || left == right)
                .count()
        };
        state.next_revision += 1;
        let revision = state.next_revision;
        state.update = Some(MarkdownRowUpdate {
            base_revision: state.revision,
            revision,
            first_changed_row,
            replacement: built
                .layout
                .rows()
                .slice(first_changed_row..built.layout.row_count()),
            reset,
        });
        state.revision = revision;
        state.stats.rows_generated += built.generated;
        state.stats.rows_reused += built.reused;
        state.layout = built.layout;
        state.layout.clone()
    }
}

struct LayoutBuild {
    layout: MarkdownLayout,
    generated: usize,
    reused: usize,
}

#[derive(Default)]
struct RowBuilder {
    store: RowStore,
    generated: usize,
    reused: usize,
}

impl RowBuilder {
    fn generated(&mut self, rows: RowChunk) {
        self.generated += rows.len();
        self.store.push(rows);
    }

    fn reused(&mut self, rows: RowChunk) {
        self.reused += rows.len();
        self.store.push(rows);
    }

    fn finish(self, document: &MarkdownDocument) -> LayoutBuild {
        LayoutBuild {
            layout: MarkdownLayout::new(self.store, document),
            generated: self.generated,
            reused: self.reused,
        }
    }
}

/// Per-presentation reuse entries for the previously laid-out document.
#[derive(Debug, Clone)]
enum CacheEntries {
    Blocks(Vec<RowChunk>),
    SourceLines(Vec<CachedSourceLine>),
}

impl Default for CacheEntries {
    fn default() -> Self {
        Self::Blocks(Vec::new())
    }
}

/// The last parsed stream plus the reuse entries built from it.
#[derive(Debug, Clone, Default)]
struct LayoutCache {
    parsed: Option<ParsedSource>,
    entries: CacheEntries,
    options: Option<MarkdownLayoutOptions>,
    theme: Option<Fingerprint>,
}

impl LayoutCache {
    fn matches(&self, options: MarkdownLayoutOptions, theme: Fingerprint) -> bool {
        self.options == Some(options) && self.theme == Some(theme)
    }

    /// True when the cached layout already reflects `stream` under `options`
    /// and `theme`, so no rendering is required.
    fn is_current(
        &self,
        stream: &MarkdownStream,
        options: MarkdownLayoutOptions,
        theme: Fingerprint,
    ) -> bool {
        self.matches(options, theme) && self.parsed.as_ref().is_some_and(|p| p.matches(stream))
    }

    /// True when no row of the previous layout can survive: a different
    /// stream identity, or different options or theme.
    fn resets_for(
        &self,
        stream: &MarkdownStream,
        options: MarkdownLayoutOptions,
        theme: Fingerprint,
    ) -> bool {
        !self.matches(options, theme)
            || self
                .parsed
                .as_ref()
                .is_none_or(|p| p.identity != stream.identity())
    }

    fn render(
        &mut self,
        stream: &MarkdownStream,
        options: MarkdownLayoutOptions,
        theme: &ReviewTheme,
        revision: Fingerprint,
        highlighter: &mut SyntaxHighlighter,
        stats: &mut MarkdownRenderStats,
    ) -> LayoutBuild {
        let reusable = self.matches(options, revision);
        let (parsed, previous) = match self.parsed.take() {
            // Same source under different options or theme: nothing to reuse.
            Some(parsed) if parsed.matches(stream) => (parsed, None),
            previous => {
                stats.parsed_bytes += stream.source().len();
                stats.parsed_documents += 1;
                (ParsedSource::parse(stream), previous.filter(|_| reusable))
            }
        };
        let cached = previous
            .as_ref()
            .map(|previous| (&*previous.document, &self.entries));
        let (rows, entries) =
            layout_document(&parsed.document, options, theme, highlighter, cached);
        let built = rows.finish(&parsed.document);
        self.parsed = Some(parsed);
        self.entries = entries;
        self.options = Some(options);
        self.theme = Some(revision);
        built
    }
}

fn layout_document(
    document: &MarkdownDocument,
    options: MarkdownLayoutOptions,
    theme: &ReviewTheme,
    highlighter: &mut SyntaxHighlighter,
    previous: Option<(&MarkdownDocument, &CacheEntries)>,
) -> (RowBuilder, CacheEntries) {
    let mut rows = RowBuilder::default();
    let entries = match options.presentation {
        MarkdownPresentation::SourceLines => {
            let cached = match previous {
                Some((_, CacheEntries::SourceLines(lines))) => lines.as_slice(),
                _ => &[],
            };
            CacheEntries::SourceLines(source_rows(
                document,
                options,
                theme,
                highlighter,
                cached,
                &mut rows,
            ))
        }
        MarkdownPresentation::Rendered => {
            let cached = match previous {
                Some((document, CacheEntries::Blocks(blocks))) => {
                    Some((document, blocks.as_slice()))
                }
                _ => None,
            };
            CacheEntries::Blocks(rendered_rows(
                document,
                options,
                theme,
                highlighter,
                cached,
                &mut rows,
            ))
        }
    };
    (rows, entries)
}

fn rendered_rows(
    document: &MarkdownDocument,
    options: MarkdownLayoutOptions,
    theme: &ReviewTheme,
    highlighter: &mut SyntaxHighlighter,
    previous: Option<(&MarkdownDocument, &[RowChunk])>,
    rows: &mut RowBuilder,
) -> Vec<RowChunk> {
    let mut blocks = Vec::with_capacity(document.blocks().len());
    let mut next_source_line = 1;
    let source_ranges = options
        .preserve_source_gaps
        .then(|| source_line_ranges(document.source()));
    for (index, block) in document.blocks().iter().enumerate() {
        if let Some(ranges) = &source_ranges {
            for line in next_source_line..block.source.lines.start {
                rows.generated(Arc::from([Arc::new(MarkdownRow {
                    line: Line::default(),
                    source: ranges.get(line - 1).cloned(),
                    target: None,
                })]));
            }
        } else if index > 0 && options.block_spacing {
            rows.generated(Arc::from([Arc::new(MarkdownRow {
                line: Line::default(),
                source: None,
                target: None,
            })]));
        }
        let cached = previous
            .filter(|(document, _)| document.blocks().get(index) == Some(block))
            .and_then(|(_, chunks)| chunks.get(index));
        let block_rows = if let Some(cached) = cached {
            rows.reused(Arc::clone(cached));
            Arc::clone(cached)
        } else {
            let mut output = RowOutput::new(options);
            render_block(
                block,
                theme,
                highlighter,
                &mut output,
                BlockContext {
                    target: None,
                    foreground: theme.diff.foreground,
                    prefix: "",
                },
            );
            let chunk: RowChunk = Arc::from(output.rows);
            rows.generated(Arc::clone(&chunk));
            chunk
        };
        blocks.push(block_rows);
        next_source_line = block.source.lines.end.saturating_add(1);
    }
    blocks
}

/// Foreground and background for fenced code, composited over the page.
fn fenced_code_style(theme: &ReviewTheme) -> Style {
    layered_style(
        theme.markdown.code,
        theme.markdown.code_background,
        theme.diff.background,
    )
}

/// Applies the inline-code role on top of `style`.
fn inline_code_style(style: Style, theme: &ReviewTheme) -> Style {
    style.patch(layered_style(
        theme.markdown.inline_code,
        theme.markdown.inline_code_background,
        theme.diff.background,
    ))
}

/// Highlights a fenced block with its complete content as parser context.
fn highlight_code_block(
    code: &MarkdownCodeBlock,
    theme: &ReviewTheme,
    highlighter: &mut SyntaxHighlighter,
) -> Arc<DocumentHighlights> {
    let lines = code
        .lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>();
    highlighter
        .with_theme(&theme.syntax)
        .highlight_document_lines(
            SourceSequenceId::from_lines(lines.iter().copied()),
            LanguageHint::InfoString(code.highlight_hint()),
            lines.iter().copied(),
        )
}

/// Ownership and styling a block inherits from its enclosing blocks.
#[derive(Clone, Copy)]
struct BlockContext<'a> {
    target: Option<MarkdownTargetId>,
    foreground: Rgba,
    prefix: &'a str,
}

/// Where the rows produced for one block element come from.
#[derive(Clone, Copy)]
struct RowOrigin<'a> {
    source: &'a SourceRange,
    target: Option<MarkdownTargetId>,
}

struct RowOutput {
    rows: Vec<Arc<MarkdownRow>>,
    options: MarkdownLayoutOptions,
}

impl RowOutput {
    const fn new(options: MarkdownLayoutOptions) -> Self {
        Self {
            rows: Vec::new(),
            options,
        }
    }

    fn push(&mut self, line: Line<'static>, origin: RowOrigin<'_>) {
        self.rows.push(Arc::new(MarkdownRow {
            line,
            source: Some(origin.source.clone()),
            target: origin.target,
        }));
    }

    fn push_wrapped(
        &mut self,
        spans: Vec<Span<'static>>,
        continuation: &str,
        origin: RowOrigin<'_>,
    ) {
        for line in fit_spans(spans, self.fit_options(self.options.width, continuation)) {
            self.push(line, origin);
        }
    }

    fn fit_options<'a>(&self, width: u16, continuation: &'a str) -> FitOptions<'a> {
        FitOptions {
            width: usize::from(width),
            wrap: self.options.wrap,
            tab_width: usize::from(self.options.tab_width),
            continuation,
        }
    }
}

fn render_block(
    block: &MarkdownBlock,
    theme: &ReviewTheme,
    highlighter: &mut SyntaxHighlighter,
    output: &mut RowOutput,
    context: BlockContext<'_>,
) {
    let prefix = context.prefix;
    let width = output.options.width;
    let origin = RowOrigin {
        source: &block.source,
        target: block.target_id.or(context.target),
    };
    match &block.kind {
        MarkdownBlockKind::Heading { level, content } => {
            let marker = if output.options.heading_markers {
                format!("{} ", "#".repeat(usize::from(*level)))
            } else {
                String::new()
            };
            let base = Style::new()
                .fg(page_color(theme, theme.markdown.heading))
                .add_modifier(Modifier::BOLD);
            let mut spans = vec![Span::styled(format!("{prefix}{marker}"), base)];
            spans.extend(inline_spans(content, base, theme));
            output.push_wrapped(spans, prefix, origin);
        }
        MarkdownBlockKind::Paragraph { content } | MarkdownBlockKind::HtmlFallback { content } => {
            let base = Style::new().fg(page_color(theme, context.foreground));
            let mut spans = vec![Span::styled(prefix.to_owned(), base)];
            spans.extend(inline_spans(content, base, theme));
            output.push_wrapped(spans, prefix, origin);
        }
        MarkdownBlockKind::List {
            ordered,
            start,
            items,
        } => render_list(
            items,
            (*ordered, *start),
            theme,
            highlighter,
            output,
            BlockContext {
                target: origin.target,
                ..context
            },
        ),
        MarkdownBlockKind::BlockQuote { blocks } => {
            let quote_prefix = format!("{prefix}│ ");
            for child in blocks {
                render_block(
                    child,
                    theme,
                    highlighter,
                    output,
                    BlockContext {
                        target: origin.target,
                        foreground: theme.markdown.quote,
                        prefix: &quote_prefix,
                    },
                );
            }
        }
        MarkdownBlockKind::CodeBlock(code) => {
            render_code(code, theme, highlighter, output, origin.target, prefix);
        }
        MarkdownBlockKind::Table(table) => {
            render_table(
                table,
                theme,
                output,
                context.foreground,
                origin.target,
                prefix,
            );
        }
        MarkdownBlockKind::Rule => output.push(
            Line::styled(
                "─".repeat(usize::from(width)),
                Style::new().fg(page_color(theme, theme.diff.border)),
            ),
            origin,
        ),
    }
}

fn render_list(
    items: &[MarkdownListItem],
    (ordered, start): (bool, Option<u64>),
    theme: &ReviewTheme,
    highlighter: &mut SyntaxHighlighter,
    output: &mut RowOutput,
    context: BlockContext<'_>,
) {
    let prefix = context.prefix;
    let base = Style::new().fg(page_color(theme, context.foreground));
    for (index, item) in items.iter().enumerate() {
        let item_target = item.target_id.or(context.target);
        let marker = if ordered {
            format!("{}.", start.unwrap_or(1).saturating_add(index as u64))
        } else {
            "•".to_owned()
        };
        let mut spans = vec![Span::styled(
            format!("{prefix}{}{marker} ", "  ".repeat(item.depth)),
            base,
        )];
        spans.extend(inline_spans(&item.content, base, theme));
        let continuation = format!("{prefix}{}", " ".repeat(marker.width() + 1));
        output.push_wrapped(
            spans,
            &continuation,
            RowOrigin {
                source: &item.source,
                target: item_target,
            },
        );
        let child_prefix = format!("{prefix}  ");
        for child in &item.blocks {
            render_block(
                child,
                theme,
                highlighter,
                output,
                BlockContext {
                    target: item_target,
                    prefix: &child_prefix,
                    ..context
                },
            );
        }
    }
}

fn render_code(
    code: &MarkdownCodeBlock,
    theme: &ReviewTheme,
    highlighter: &mut SyntaxHighlighter,
    output: &mut RowOutput,
    target: Option<MarkdownTargetId>,
    prefix: &str,
) {
    let highlights = highlight_code_block(code, theme, highlighter);
    let base = fenced_code_style(theme);
    for (index, line) in code.lines.iter().enumerate() {
        let mut rendered =
            highlighted_line(&line.text, highlights.line(index).unwrap_or_default(), base);
        rendered
            .spans
            .insert(0, Span::styled(prefix.to_owned(), base));
        output.push_wrapped(
            rendered.spans,
            prefix,
            RowOrigin {
                source: &line.source,
                target: line.target_id.or(target),
            },
        );
    }
}

/// Column widths for `table`, or `None` when the columns cannot fit side by
/// side and cells must stack.
fn table_column_widths(table: &MarkdownTable, available: usize, wrap: bool) -> Option<Vec<usize>> {
    let columns = table_columns(table);
    let mut natural = vec![1; columns];
    for row in &table.rows {
        for (index, cell) in row.cells.iter().enumerate() {
            natural[index] = natural[index].max(rendered_text(&cell.content).width());
        }
    }
    if !wrap || natural.iter().sum::<usize>() <= available {
        return Some(natural);
    }
    if available < columns {
        return None;
    }
    let mut order = (0..columns).collect::<Vec<_>>();
    order.sort_by_key(|index| natural[*index]);
    let mut widths = vec![0; columns];
    let mut budget = available;
    for (rank, index) in order.into_iter().enumerate() {
        let share = budget / (columns - rank);
        widths[index] = natural[index].min(share);
        budget -= widths[index];
    }
    Some(widths)
}

fn table_columns(table: &MarkdownTable) -> usize {
    table
        .rows
        .iter()
        .map(|row| row.cells.len())
        .max()
        .unwrap_or(0)
}

fn render_table(
    table: &MarkdownTable,
    theme: &ReviewTheme,
    output: &mut RowOutput,
    foreground: Rgba,
    target: Option<MarkdownTargetId>,
    prefix: &str,
) {
    let text = Style::new().fg(page_color(theme, foreground));
    let border = Style::new().fg(page_color(theme, theme.diff.border));
    let columns = table_columns(table);
    let available =
        usize::from(output.options.width).saturating_sub(prefix.width() + columns * 3 + 1);
    let widths = table_column_widths(table, available, output.options.wrap);
    for row in &table.rows {
        let base = if row.header {
            text.add_modifier(Modifier::BOLD)
        } else {
            text
        };
        let origin = RowOrigin {
            source: &row.source,
            target: row.target_id.or(target),
        };
        let Some(widths) = &widths else {
            for cell in &row.cells {
                output.push_wrapped(inline_spans(&cell.content, base, theme), prefix, origin);
            }
            continue;
        };
        if widths.is_empty() {
            continue;
        }
        let cells = row
            .cells
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                fit_spans(
                    inline_spans(&cell.content, base, theme),
                    output.fit_options(u16::try_from(widths[index]).unwrap_or(u16::MAX), ""),
                )
            })
            .collect::<Vec<_>>();
        let height = cells.iter().map(Vec::len).max().unwrap_or(1);
        for line in 0..height {
            let mut spans = vec![Span::styled(format!("{prefix}│ "), border)];
            for (index, cell_width) in widths.iter().copied().enumerate() {
                if index > 0 {
                    spans.push(Span::styled(" │ ", border));
                }
                let cell = cells.get(index).and_then(|rows| rows.get(line));
                let padding = cell_width.saturating_sub(cell.map_or(0, Line::width));
                let left = match table.alignments.get(index) {
                    Some(MarkdownTableAlignment::Right) => padding,
                    Some(MarkdownTableAlignment::Center) => padding / 2,
                    _ => 0,
                };
                spans.push(Span::styled(" ".repeat(left), base));
                if let Some(cell) = cell {
                    spans.extend(cell.spans.iter().cloned());
                }
                spans.push(Span::styled(" ".repeat(padding - left), base));
            }
            spans.push(Span::styled(" │", border));
            output.push_wrapped(spans, prefix, origin);
        }
    }
}

fn inline_spans(
    inlines: &[MarkdownInline],
    style: Style,
    theme: &ReviewTheme,
) -> Vec<Span<'static>> {
    fn append(
        inline: &MarkdownInline,
        style: Style,
        theme: &ReviewTheme,
        output: &mut Vec<Span<'static>>,
    ) {
        match inline {
            MarkdownInline::Text(text) => output.push(Span::styled(text.clone(), style)),
            MarkdownInline::Code(text) => {
                output.push(Span::styled(text.clone(), inline_code_style(style, theme)));
            }
            MarkdownInline::Strong(children) => children.iter().for_each(|child| {
                append(child, style.add_modifier(Modifier::BOLD), theme, output);
            }),
            MarkdownInline::Emphasis(children) => children.iter().for_each(|child| {
                append(child, style.add_modifier(Modifier::ITALIC), theme, output);
            }),
            MarkdownInline::Strikethrough(children) => children.iter().for_each(|child| {
                append(
                    child,
                    style.add_modifier(Modifier::CROSSED_OUT),
                    theme,
                    output,
                );
            }),
            MarkdownInline::Link { content, .. } => content.iter().for_each(|child| {
                append(
                    child,
                    style
                        .fg(page_color(theme, theme.markdown.link))
                        .add_modifier(Modifier::UNDERLINED),
                    theme,
                    output,
                );
            }),
            MarkdownInline::SoftBreak => output.push(Span::styled(" ", style)),
            MarkdownInline::HardBreak => output.push(Span::styled("\n", style)),
            MarkdownInline::ImageAlt(text) => output.push(Span::styled(
                format!("Image: {text}"),
                style
                    .fg(page_color(theme, theme.markdown.link))
                    .add_modifier(Modifier::ITALIC),
            )),
        }
    }

    let mut output = Vec::new();
    for inline in inlines {
        append(inline, style, theme, &mut output);
    }
    output
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceLineKey {
    text: String,
    source: SourceRange,
    target: Option<MarkdownTargetId>,
    styles: Vec<MarkdownSourceStyle>,
    code: Option<(String, Arc<[HighlightSpan]>)>,
}

impl SourceLineKey {
    fn matches(
        &self,
        text: &str,
        source: &SourceRange,
        target: Option<MarkdownTargetId>,
        styles: &[&MarkdownSourceStyle],
        code: Option<&(&str, Arc<[HighlightSpan]>)>,
    ) -> bool {
        self.text == text
            && self.source == *source
            && self.target == target
            && self.styles.iter().eq(styles.iter().copied())
            && self
                .code
                .as_ref()
                .map(|(text, spans)| (text.as_str(), spans))
                == code.map(|(text, spans)| (*text, spans))
    }
}

#[derive(Debug, Clone)]
struct CachedSourceLine {
    key: Arc<SourceLineKey>,
    rows: RowChunk,
}

fn source_rows(
    document: &MarkdownDocument,
    options: MarkdownLayoutOptions,
    theme: &ReviewTheme,
    highlighter: &mut SyntaxHighlighter,
    cached: &[CachedSourceLine],
    rows: &mut RowBuilder,
) -> Vec<CachedSourceLine> {
    let source = document.source();
    let lines = source_line_ranges(source);
    let code_style = fenced_code_style(theme);
    let code_lines = source_code_lines(document, theme, highlighter);
    let mut styles_by_line: Vec<Vec<&MarkdownSourceStyle>> = vec![Vec::new(); lines.len()];
    for style in document.source_styles() {
        for line in style.source.lines.start..=style.source.lines.end {
            if let Some(bucket) = styles_by_line.get_mut(line.wrapping_sub(1)) {
                bucket.push(style);
            }
        }
    }
    let mut target_by_line: Vec<Option<(usize, MarkdownTargetId)>> = vec![None; lines.len()];
    for target in document.targets() {
        for line in target.source.lines.start..=target.source.lines.end {
            if let Some(slot) = target_by_line.get_mut(line.wrapping_sub(1)) {
                let candidate = (target.source.bytes.len(), target.id);
                if slot.is_none_or(|current| candidate.0 < current.0) {
                    *slot = Some(candidate);
                }
            }
        }
    }
    let base = Style::new().fg(page_color(theme, theme.diff.foreground));
    let mut result = Vec::with_capacity(lines.len());
    for (index, range) in lines.iter().enumerate() {
        let raw = &source[range.bytes.clone()];
        let text = raw.strip_suffix('\n').unwrap_or(raw);
        let text = text.strip_suffix('\r').unwrap_or(text);
        let target = target_by_line[index].map(|(_, id)| id);
        let code_input = code_lines.get(&(index + 1));
        let styles = &styles_by_line[index];
        if let Some(cached) = cached
            .get(index)
            .filter(|cached| cached.key.matches(text, range, target, styles, code_input))
        {
            rows.reused(Arc::clone(&cached.rows));
            result.push(cached.clone());
            continue;
        }
        let mut output = RowOutput::new(options);
        let code = code_input.and_then(|(code, spans)| {
            Some((
                text.strip_suffix(code)?,
                highlighted_line(code, spans, code_style),
            ))
        });
        let spans = match code {
            Some((prefix, line)) => {
                let mut spans = vec![Span::styled(prefix.to_owned(), code_style)];
                spans.extend(line.spans.iter().cloned());
                spans
            }
            None => text
                .grapheme_indices(true)
                .map(|(offset, grapheme)| {
                    let position = range.bytes.start + offset;
                    let style = styles
                        .iter()
                        .filter(|style| style.source.bytes.contains(&position))
                        .fold(base, |style, role| {
                            source_role_style(role.role, style, theme)
                        });
                    Span::styled(grapheme.to_owned(), style)
                })
                .collect(),
        };
        output.push_wrapped(
            spans,
            "",
            RowOrigin {
                source: range,
                target,
            },
        );
        let chunk: RowChunk = output.rows.into();
        rows.generated(Arc::clone(&chunk));
        result.push(CachedSourceLine {
            key: Arc::new(SourceLineKey {
                text: text.to_owned(),
                source: range.clone(),
                target,
                styles: styles.iter().copied().cloned().collect(),
                code: code_input.map(|(text, spans)| ((*text).to_owned(), Arc::clone(spans))),
            }),
            rows: chunk,
        });
    }
    result
}

/// Byte and line ranges of every source line, including its line ending.
fn source_line_ranges(source: &str) -> Vec<SourceRange> {
    let mut start = 0;
    source
        .split('\n')
        .enumerate()
        .map(|(index, text)| {
            let end = (start + text.len() + 1).min(source.len());
            let range = SourceRange {
                bytes: start..end,
                lines: MarkdownLineRange {
                    start: index + 1,
                    end: index + 1,
                },
            };
            start = end;
            range
        })
        .collect()
}

fn source_code_lines<'a>(
    document: &'a MarkdownDocument,
    theme: &ReviewTheme,
    highlighter: &mut SyntaxHighlighter,
) -> HashMap<usize, (&'a str, Arc<[HighlightSpan]>)> {
    let mut lines = HashMap::new();
    for code in document.code_blocks() {
        let highlights = highlight_code_block(code, theme, highlighter);
        for (index, line) in code.lines.iter().enumerate() {
            if let Some(source_line) = line.source_line {
                lines.insert(
                    source_line,
                    (
                        line.text.as_str(),
                        highlights.line_shared(index).unwrap_or_default(),
                    ),
                );
            }
        }
    }
    lines
}

fn source_role_style(role: MarkdownSourceRole, style: Style, theme: &ReviewTheme) -> Style {
    match role {
        MarkdownSourceRole::Heading => style
            .fg(page_color(theme, theme.markdown.heading))
            .add_modifier(Modifier::BOLD),
        MarkdownSourceRole::Link => style
            .fg(page_color(theme, theme.markdown.link))
            .add_modifier(Modifier::UNDERLINED),
        MarkdownSourceRole::Quote => style.fg(page_color(theme, theme.markdown.quote)),
        MarkdownSourceRole::Code => inline_code_style(style, theme),
        MarkdownSourceRole::Strong => style.add_modifier(Modifier::BOLD),
        MarkdownSourceRole::Emphasis => style.add_modifier(Modifier::ITALIC),
        MarkdownSourceRole::Strikethrough => style.add_modifier(Modifier::CROSSED_OUT),
    }
}
