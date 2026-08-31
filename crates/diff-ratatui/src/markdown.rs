//! Read-only whole-document and append-stream Markdown rendering.

use crate::syntax::highlighted_line;
use diff_markdown::{
    FenceContinuation, MarkdownBlock, MarkdownBlockKind, MarkdownDocument, MarkdownInline,
    MarkdownStream,
};
use diff_syntax::{LanguageHint, SyntaxHighlighter, SyntaxStream};
use diff_theme::{Fingerprint, HighlightSpan, ReviewTheme, Rgba};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::{collections::VecDeque, sync::Arc};
use unicode_width::UnicodeWidthChar;

/// Width and spacing policy for transcript-style Markdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkdownRenderOptions {
    pub width: u16,
    pub block_spacing: bool,
}

impl Default for MarkdownRenderOptions {
    fn default() -> Self {
        Self {
            width: 80,
            block_spacing: true,
        }
    }
}

/// Deterministic work counters for incremental Markdown rendering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarkdownRenderStats {
    /// Bytes actually supplied to the Markdown parser.
    pub parsed_bytes: usize,
    pub stable_segments: u64,
    pub speculative_segments: u64,
}

/// One committed code line retained for bounded speculative restyling.
#[derive(Debug, Clone)]
struct ContextLine {
    text: String,
    rows: Vec<Line<'static>>,
}

#[derive(Debug, Clone)]
struct StreamingFenceState {
    continuation: FenceContinuation,
    /// First source byte not yet appended to `syntax`.
    content_offset: usize,
    /// Emit one blank row before the block's first code row.
    leading_gap: bool,
    /// Whether rows were already moved into committed output (gap included).
    flushed_rows: bool,
    /// Trailing committed code lines and their committed rows. These stay out
    /// of the committed row list so the speculative tail can restyle them
    /// without truncating committed output.
    context: VecDeque<ContextLine>,
    syntax: SyntaxStream,
}

/// Renderer-owned cache for one logical streaming Markdown item.
#[derive(Debug, Clone, Default)]
pub struct StreamingMarkdownState {
    revision: Option<u64>,
    stream_generation: Option<u64>,
    options: MarkdownRenderOptions,
    theme_revision: Fingerprint,
    stable_offset: usize,
    stable_lines: Vec<Line<'static>>,
    open_fence: Option<StreamingFenceState>,
    lines: Arc<[Line<'static>]>,
    stats: MarkdownRenderStats,
}

impl StreamingMarkdownState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Returns accumulated renderer work counters and resets them.
    pub fn take_stats(&mut self) -> MarkdownRenderStats {
        std::mem::take(&mut self.stats)
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
        options: MarkdownRenderOptions,
        theme: &ReviewTheme,
        highlighter: &mut SyntaxHighlighter,
    ) -> Arc<[Line<'static>]> {
        Arc::from(document_rows(document, options, theme, highlighter))
    }

    /// Renders the current stream snapshot while retaining rows produced from
    /// the stream's stable source prefix. Width, theme, replacement, or a
    /// backwards-moving stable offset invalidates the retained prefix.
    pub fn render_stream_lines(
        &self,
        state: &mut StreamingMarkdownState,
        stream: &MarkdownStream,
        options: MarkdownRenderOptions,
        theme: &ReviewTheme,
        highlighter: &mut SyntaxHighlighter,
    ) -> Arc<[Line<'static>]> {
        let revision = stream.revision();
        let theme_revision = theme.revision();
        if state.revision == Some(revision)
            && state.options == options
            && state.theme_revision == theme_revision
        {
            return Arc::clone(&state.lines);
        }
        let stable_offset = stream.stable_offset().min(stream.source().len());
        let cache_invalid = state.revision.is_none()
            || state.stream_generation != Some(stream.generation())
            || state.options != options
            || state.theme_revision != theme_revision
            || stable_offset < state.stable_offset;
        if cache_invalid {
            state.stable_offset = 0;
            state.stable_lines.clear();
            state.open_fence = None;
        }

        let mut ctx = RenderCtx {
            options,
            theme,
            highlighter,
        };
        if stable_offset > state.stable_offset {
            commit_stable_segment(&mut ctx, state, stream, stable_offset);
        }

        let mut combined = state.stable_lines.clone();
        let tail = &stream.source()[stable_offset..];
        if let Some(fence) = &state.open_fence {
            let rows = if tail.is_empty() {
                fence
                    .context
                    .iter()
                    .flat_map(|line| line.rows.iter().cloned())
                    .collect()
            } else {
                state.stats.speculative_segments =
                    state.stats.speculative_segments.saturating_add(1);
                speculative_fence_rows(&mut ctx, fence, tail, &mut state.stats)
            };
            if !rows.is_empty() {
                if fence.leading_gap && !fence.flushed_rows {
                    combined.push(Line::default());
                }
                combined.extend(rows);
            }
        } else if !tail.is_empty() {
            state.stats.speculative_segments = state.stats.speculative_segments.saturating_add(1);
            let rows = parsed_rows(&mut ctx, tail, &mut state.stats);
            append_segment(&mut combined, rows, options.block_spacing);
        }
        let lines = Arc::from(combined);
        state.revision = Some(revision);
        state.stream_generation = Some(stream.generation());
        state.options = options;
        state.theme_revision = theme_revision;
        state.lines = Arc::clone(&lines);
        lines
    }
}

/// Shared parameters threaded through the streaming render helpers.
struct RenderCtx<'a> {
    options: MarkdownRenderOptions,
    theme: &'a ReviewTheme,
    highlighter: &'a mut SyntaxHighlighter,
}

fn commit_stable_segment(
    ctx: &mut RenderCtx<'_>,
    state: &mut StreamingMarkdownState,
    stream: &MarkdownStream,
    stable_offset: usize,
) {
    state.stats.stable_segments = state.stats.stable_segments.saturating_add(1);
    let source = stream.source();
    let mut offset = state.stable_offset;
    if let Some(mut fence) = state.open_fence.take() {
        let segment = &source[fence.content_offset..stable_offset];
        let still_open = stream.continuation().is_some_and(|continuation| {
            continuation.opening_line() == fence.continuation.opening_line()
        });
        if still_open {
            append_fence_lines(ctx, &mut fence, segment, &mut state.stable_lines);
            fence.content_offset = stable_offset;
            state.open_fence = Some(fence);
            state.stable_offset = stable_offset;
            return;
        }
        if let Some(close) = fence.continuation.find_close(segment) {
            let closing = fence_append_rows(ctx, &mut fence, &segment[..close.start]);
            let mut rows: Vec<Line<'static>> =
                fence.context.drain(..).flat_map(|line| line.rows).collect();
            rows.extend(closing.into_iter().flat_map(|(_, line_rows)| line_rows));
            flush_fence_rows(&mut state.stable_lines, &mut fence, rows);
            offset = fence.content_offset + close.end;
        } else {
            // `finish()` can stabilize an unclosed fence. Restyle the retained
            // bounded context together with the final partial content so
            // multiline constructs discovered late still render correctly.
            let rows = restyled_fence_rows(ctx, &fence, segment);
            flush_fence_rows(&mut state.stable_lines, &mut fence, rows);
            offset = stable_offset;
        }
    }
    if let Some(continuation) = stream.continuation() {
        let opening = continuation.opening_line();
        let prose = &source[offset.min(opening)..opening];
        if !prose.is_empty() {
            let rows = parsed_rows(ctx, prose, &mut state.stats);
            append_segment(&mut state.stable_lines, rows, ctx.options.block_spacing);
        }
        let mut fence = StreamingFenceState {
            continuation: continuation.clone(),
            content_offset: stable_offset,
            leading_gap: ctx.options.block_spacing && !state.stable_lines.is_empty(),
            flushed_rows: false,
            context: VecDeque::new(),
            syntax: SyntaxStream::new(LanguageHint::InfoString(continuation.info_string())),
        };
        let code = &source[continuation.content_start()..stable_offset];
        append_fence_lines(ctx, &mut fence, code, &mut state.stable_lines);
        state.open_fence = Some(fence);
    } else {
        let segment = &source[offset..stable_offset];
        if !segment.is_empty() {
            let rows = parsed_rows(ctx, segment, &mut state.stats);
            append_segment(&mut state.stable_lines, rows, ctx.options.block_spacing);
        }
    }
    state.stable_offset = stable_offset;
}

/// Renders the unstable tail of an open fence from restyled retained context,
/// plus any prose following a speculative closing fence.
fn speculative_fence_rows(
    ctx: &mut RenderCtx<'_>,
    fence: &StreamingFenceState,
    tail: &str,
    stats: &mut MarkdownRenderStats,
) -> Vec<Line<'static>> {
    let (code, prose) = match fence.continuation.find_close(tail) {
        Some(close) => (&tail[..close.start], &tail[close.end..]),
        None => (tail, ""),
    };
    let mut rows = restyled_fence_rows(ctx, fence, code);
    if !prose.is_empty() {
        let prose_rows = parsed_rows(ctx, prose, stats);
        append_segment(&mut rows, prose_rows, ctx.options.block_spacing);
    }
    rows
}

/// Statelessly re-highlights the retained context lines together with `code`,
/// so partial content can restyle earlier lines without touching the fence's
/// committed syntax stream.
fn restyled_fence_rows(
    ctx: &mut RenderCtx<'_>,
    fence: &StreamingFenceState,
    code: &str,
) -> Vec<Line<'static>> {
    let mut lines: Vec<&str> = fence
        .context
        .iter()
        .map(|line| line.text.as_str())
        .collect();
    lines.extend(code.lines());
    if lines.is_empty() {
        return Vec::new();
    }
    let line_spans = ctx
        .highlighter
        .with_theme(&ctx.theme.syntax)
        .highlight_lines(
            LanguageHint::InfoString(fence.continuation.info_string()),
            lines.iter().copied(),
        );
    code_rows(&lines, line_spans, ctx.theme, ctx.options.width, "")
}

/// Appends committed code to the fence's syntax stream, retains the bounded
/// trailing stream history, and flushes rows that leave the retention bound.
fn append_fence_lines(
    ctx: &mut RenderCtx<'_>,
    fence: &mut StreamingFenceState,
    code: &str,
    output: &mut Vec<Line<'static>>,
) {
    for (text, rows) in fence_append_rows(ctx, fence, code) {
        fence.context.push_back(ContextLine { text, rows });
    }
    let retained = ctx.highlighter.config().max_stream_lines;
    while fence.context.len() > retained {
        let line = fence.context.pop_front().expect("length checked above");
        flush_fence_rows(output, fence, line.rows);
    }
}

/// Appends code lines to the fence's syntax stream, returning each line with
/// its rendered rows.
fn fence_append_rows(
    ctx: &mut RenderCtx<'_>,
    fence: &mut StreamingFenceState,
    code: &str,
) -> Vec<(String, Vec<Line<'static>>)> {
    let lines: Vec<&str> = code.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }
    let line_spans = ctx
        .highlighter
        .with_theme(&ctx.theme.syntax)
        .append(&mut fence.syntax, lines.iter().copied());
    lines
        .into_iter()
        .zip(line_spans)
        .map(|(line, spans)| {
            let rows = code_rows(&[line], vec![spans], ctx.theme, ctx.options.width, "");
            (line.to_owned(), rows)
        })
        .collect()
}

fn flush_fence_rows(
    output: &mut Vec<Line<'static>>,
    fence: &mut StreamingFenceState,
    rows: Vec<Line<'static>>,
) {
    if rows.is_empty() {
        return;
    }
    if fence.leading_gap && !fence.flushed_rows {
        output.push(Line::default());
    }
    fence.flushed_rows = true;
    output.extend(rows);
}

fn parsed_rows(
    ctx: &mut RenderCtx<'_>,
    source: &str,
    stats: &mut MarkdownRenderStats,
) -> Vec<Line<'static>> {
    stats.parsed_bytes = stats.parsed_bytes.saturating_add(source.len());
    document_rows(
        &MarkdownDocument::parse(source),
        ctx.options,
        ctx.theme,
        ctx.highlighter,
    )
}

fn document_rows(
    document: &MarkdownDocument,
    options: MarkdownRenderOptions,
    theme: &ReviewTheme,
    highlighter: &mut SyntaxHighlighter,
) -> Vec<Line<'static>> {
    let mut output = Vec::new();
    for (index, block) in document.blocks().iter().enumerate() {
        render_block(
            block,
            options.width.max(1),
            theme,
            highlighter,
            &mut output,
            "",
        );
        if options.block_spacing && index + 1 < document.blocks().len() {
            output.push(Line::default());
        }
    }
    output
}

/// Renders highlighted code lines as wrapped rows on the code background.
fn code_rows(
    lines: &[&str],
    line_spans: Vec<Vec<HighlightSpan>>,
    theme: &ReviewTheme,
    width: u16,
    prefix: &str,
) -> Vec<Line<'static>> {
    let base = Style::new()
        .fg(color(theme.markdown.code))
        .bg(color(theme.diff.background));
    let mut output = Vec::new();
    for (line, spans) in lines.iter().zip(line_spans) {
        let mut rendered = highlighted_line(line, &spans, base);
        if !prefix.is_empty() {
            rendered
                .spans
                .insert(0, Span::styled(prefix.to_owned(), base));
        }
        push_wrapped_spans(&mut output, rendered.spans, width);
    }
    output
}

fn append_segment(
    output: &mut Vec<Line<'static>>,
    segment: Vec<Line<'static>>,
    block_spacing: bool,
) {
    if block_spacing && !output.is_empty() && !segment.is_empty() {
        output.push(Line::default());
    }
    output.extend(segment);
}

fn render_block(
    block: &MarkdownBlock,
    width: u16,
    theme: &ReviewTheme,
    highlighter: &mut SyntaxHighlighter,
    output: &mut Vec<Line<'static>>,
    prefix: &str,
) {
    match &block.kind {
        MarkdownBlockKind::Heading { level, content } => {
            let marker = format!("{} ", "#".repeat(usize::from(*level)));
            let base = Style::new()
                .fg(color(theme.markdown.heading))
                .add_modifier(Modifier::BOLD);
            let mut spans = vec![Span::styled(format!("{prefix}{marker}"), base)];
            spans.extend(inline_spans(content, base, theme));
            push_wrapped_spans(output, spans, width);
        }
        MarkdownBlockKind::Paragraph { content } | MarkdownBlockKind::HtmlFallback { content } => {
            let base = Style::new().fg(color(theme.diff.foreground));
            let mut spans = vec![Span::styled(prefix.to_owned(), base)];
            spans.extend(inline_spans(content, base, theme));
            push_wrapped_spans(output, spans, width);
        }
        MarkdownBlockKind::List {
            ordered,
            start,
            items,
        } => {
            for (index, item) in items.iter().enumerate() {
                let marker = if *ordered {
                    format!("{}.", start.unwrap_or(1).saturating_add(index as u64))
                } else {
                    "•".to_owned()
                };
                let base = Style::new().fg(color(theme.diff.foreground));
                let mut spans = vec![Span::styled(
                    format!("{prefix}{}{marker} ", "  ".repeat(item.depth)),
                    base,
                )];
                spans.extend(inline_spans(&item.content, base, theme));
                push_wrapped_spans(output, spans, width);
                for child in &item.blocks {
                    render_block(
                        child,
                        width,
                        theme,
                        highlighter,
                        output,
                        &format!("{prefix}  "),
                    );
                }
            }
        }
        MarkdownBlockKind::BlockQuote { blocks } => {
            for child in blocks {
                render_block(
                    child,
                    width,
                    theme,
                    highlighter,
                    output,
                    &format!("{prefix}│ "),
                );
            }
        }
        MarkdownBlockKind::CodeBlock(code) => {
            let source = code
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>();
            let line_spans = highlighter.with_theme(&theme.syntax).highlight_lines(
                LanguageHint::InfoString(code.highlight_hint()),
                source.iter().copied(),
            );
            output.extend(code_rows(&source, line_spans, theme, width, prefix));
        }
        MarkdownBlockKind::Table(table) => {
            for row in &table.rows {
                let base = Style::new().fg(color(theme.diff.foreground));
                let border = Style::new().fg(color(theme.diff.border));
                let mut spans = vec![Span::styled(format!("{prefix}│ "), border)];
                for (index, cell) in row.cells.iter().enumerate() {
                    if index > 0 {
                        spans.push(Span::styled(" │ ", border));
                    }
                    let cell_base = if row.header {
                        base.add_modifier(Modifier::BOLD)
                    } else {
                        base
                    };
                    spans.extend(inline_spans(&cell.content, cell_base, theme));
                }
                spans.push(Span::styled(" │", border));
                push_wrapped_spans(output, spans, width);
            }
        }
        MarkdownBlockKind::Rule => output.push(Line::styled(
            "─".repeat(usize::from(width)),
            Style::new().fg(color(theme.diff.border)),
        )),
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
            MarkdownInline::Code(text) => output.push(Span::styled(
                text.clone(),
                style
                    .fg(color(theme.markdown.code))
                    .bg(color(theme.diff.border)),
            )),
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
                        .fg(color(theme.markdown.link))
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
                    .fg(color(theme.markdown.link))
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

fn push_wrapped_spans(output: &mut Vec<Line<'static>>, spans: Vec<Span<'static>>, width: u16) {
    let width = usize::from(width.max(1));
    let mut rows = vec![Line::default()];
    let mut used = 0usize;
    for span in spans {
        let style = span.style;
        for character in span.content.chars() {
            if character == '\n' {
                rows.push(Line::default());
                used = 0;
                continue;
            }
            let character_width = character.width().unwrap_or(0);
            if used > 0 && used.saturating_add(character_width) > width {
                rows.push(Line::default());
                used = 0;
            }
            let row = rows.last_mut().expect("wrapping always retains one row");
            if let Some(last) = row.spans.last_mut().filter(|last| last.style == style) {
                last.content.to_mut().push(character);
            } else {
                row.spans.push(Span::styled(character.to_string(), style));
            }
            used = used.saturating_add(character_width);
        }
    }
    output.extend(rows);
}

const fn color(value: Rgba) -> Color {
    Color::Rgb(value.r, value.g, value.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finished_stream_matches_one_shot_for_every_utf8_chunk_boundary() {
        let source = "# Héading\n\nText with 世界.\n\n```rust\n/* open\nstill comment */\n```\n";
        let options = MarkdownRenderOptions {
            width: 36,
            block_spacing: true,
        };
        let theme = ReviewTheme::default();
        let expected = MarkdownRenderer::new().render_lines(
            &MarkdownDocument::parse(source),
            options,
            &theme,
            &mut SyntaxHighlighter::default(),
        );

        for split in source
            .char_indices()
            .map(|(offset, _)| offset)
            .chain(std::iter::once(source.len()))
        {
            let mut stream = MarkdownStream::new();
            stream.push(&source[..split]);
            stream.push(&source[split..]);
            stream.finish();
            let actual = MarkdownRenderer::new().render_stream_lines(
                &mut StreamingMarkdownState::default(),
                &stream,
                options,
                &theme,
                &mut SyntaxHighlighter::default(),
            );
            assert_eq!(actual, expected, "split at byte {split}");
        }
    }

    #[test]
    fn open_fence_stream_matches_the_current_one_shot_snapshot() {
        let source = "Settled prose.\n\n```rust\n/* open\nstill open";
        let mut stream = MarkdownStream::new();
        for chunk in ["Settled prose.\n\n```rust\n", "/* open\n", "still open"] {
            stream.push(chunk);
        }
        assert!(stream.continuation().is_some());
        let options = MarkdownRenderOptions {
            width: 48,
            block_spacing: true,
        };
        let theme = ReviewTheme::default();
        let expected = MarkdownRenderer::new().render_lines(
            &MarkdownDocument::parse(source),
            options,
            &theme,
            &mut SyntaxHighlighter::default(),
        );
        let actual = MarkdownRenderer::new().render_stream_lines(
            &mut StreamingMarkdownState::default(),
            &stream,
            options,
            &theme,
            &mut SyntaxHighlighter::default(),
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn finishing_an_open_fence_commits_its_partial_final_line() {
        let source = "```rust\n/* open\nstill open";
        let mut stream = MarkdownStream::new();
        stream.push(source);
        let mut state = StreamingMarkdownState::default();
        let renderer = MarkdownRenderer::new();
        let options = MarkdownRenderOptions::default();
        let theme = ReviewTheme::default();
        let mut highlighter = SyntaxHighlighter::default();
        renderer.render_stream_lines(&mut state, &stream, options, &theme, &mut highlighter);
        stream.finish();
        let actual =
            renderer.render_stream_lines(&mut state, &stream, options, &theme, &mut highlighter);
        let expected = renderer.render_lines(
            &MarkdownDocument::parse(source),
            options,
            &theme,
            &mut SyntaxHighlighter::default(),
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn unchanged_stream_reuses_rendered_lines() {
        let mut stream = MarkdownStream::new();
        stream.push("settled\n\n");
        let mut state = StreamingMarkdownState::default();
        let renderer = MarkdownRenderer::new();
        let mut highlighter = SyntaxHighlighter::default();
        let theme = ReviewTheme::default();
        let first = renderer.render_stream_lines(
            &mut state,
            &stream,
            MarkdownRenderOptions::default(),
            &theme,
            &mut highlighter,
        );
        let second = renderer.render_stream_lines(
            &mut state,
            &stream,
            MarkdownRenderOptions::default(),
            &theme,
            &mut highlighter,
        );
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn read_only_rendering_preserves_nested_inline_styles() {
        let source = "**bold and *both***, ~~gone~~, [`link`](https://example.com), `code`.";
        let lines = MarkdownRenderer::new().render_lines(
            &MarkdownDocument::parse(source),
            MarkdownRenderOptions::default(),
            &ReviewTheme::default(),
            &mut SyntaxHighlighter::default(),
        );
        let spans = &lines[0].spans;
        let both = spans
            .iter()
            .find(|span| span.content.contains("both"))
            .unwrap();
        assert!(both.style.add_modifier.contains(Modifier::BOLD));
        assert!(both.style.add_modifier.contains(Modifier::ITALIC));
        let gone = spans
            .iter()
            .find(|span| span.content.contains("gone"))
            .unwrap();
        assert!(gone.style.add_modifier.contains(Modifier::CROSSED_OUT));
        let link = spans
            .iter()
            .find(|span| span.content.contains("link"))
            .unwrap();
        assert!(link.style.add_modifier.contains(Modifier::UNDERLINED));
        let code = spans
            .iter()
            .find(|span| span.content.contains("code"))
            .unwrap();
        assert!(code.style.bg.is_some());
    }

    #[test]
    fn stream_stats_reset_and_stable_prose_is_not_reparsed_from_zero() {
        let renderer = MarkdownRenderer::new();
        let theme = ReviewTheme::default();
        let options = MarkdownRenderOptions::default();
        let mut stream = MarkdownStream::new();
        let mut state = StreamingMarkdownState::default();
        let mut highlighter = SyntaxHighlighter::default();
        stream.push("first paragraph\n\n");
        renderer.render_stream_lines(&mut state, &stream, options, &theme, &mut highlighter);
        let first = state.take_stats();
        assert_eq!(first.parsed_bytes, "first paragraph\n\n".len());
        stream.push("second paragraph\n\n");
        renderer.render_stream_lines(&mut state, &stream, options, &theme, &mut highlighter);
        let second = state.take_stats();
        assert_eq!(second.parsed_bytes, "second paragraph\n\n".len());
        assert_eq!(state.take_stats(), MarkdownRenderStats::default());

        stream.replace("replacement is longer than both prior paragraphs\n\n");
        let replaced =
            renderer.render_stream_lines(&mut state, &stream, options, &theme, &mut highlighter);
        let expected = renderer.render_lines(
            &MarkdownDocument::parse(stream.source()),
            options,
            &theme,
            &mut SyntaxHighlighter::default(),
        );
        assert_eq!(replaced, expected);
        assert_eq!(state.take_stats().parsed_bytes, stream.source().len());
    }

    #[test]
    fn stream_cache_invalidates_for_spacing_and_markdown_palette_changes() {
        let mut stream = MarkdownStream::new();
        stream.push("# Heading\n\nParagraph\n\n");
        let mut state = StreamingMarkdownState::default();
        let renderer = MarkdownRenderer::new();
        let mut highlighter = SyntaxHighlighter::default();
        let theme = ReviewTheme::default();
        let spaced = renderer.render_stream_lines(
            &mut state,
            &stream,
            MarkdownRenderOptions {
                width: 80,
                block_spacing: true,
            },
            &theme,
            &mut highlighter,
        );
        let compact = renderer.render_stream_lines(
            &mut state,
            &stream,
            MarkdownRenderOptions {
                width: 80,
                block_spacing: false,
            },
            &theme,
            &mut highlighter,
        );
        assert_ne!(spaced, compact);

        let mut changed = theme.clone();
        changed.markdown.heading = Rgba::new(1, 2, 3, 255);
        let recolored = renderer.render_stream_lines(
            &mut state,
            &stream,
            MarkdownRenderOptions {
                width: 80,
                block_spacing: false,
            },
            &changed,
            &mut highlighter,
        );
        assert_ne!(compact, recolored);
    }
}
