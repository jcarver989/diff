//! Read-only whole-document and append-stream Markdown rendering.

use crate::syntax::highlighted_line;
use clankerdiff_markdown::{
    MarkdownBlock, MarkdownBlockKind, MarkdownDocument, MarkdownInline, MarkdownStream,
    MarkdownStreamIdentity,
};
use clankerdiff_syntax::{DocumentHighlights, LanguageHint, SourceSequenceId, SyntaxHighlighter};
use clankerdiff_theme::{Fingerprint, ReviewTheme, Rgba};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::sync::Arc;
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
    pub parsed_documents: u64,
    pub rows_generated: usize,
    pub rows_reused: usize,
}

/// Renderer-owned cache for one logical streaming Markdown item.
#[derive(Debug, Clone, Default)]
pub struct StreamingMarkdownState {
    parsed: Option<ParsedSource>,
    options: MarkdownRenderOptions,
    theme_revision: Fingerprint,
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

#[derive(Debug, Clone)]
struct ParsedSource {
    identity: MarkdownStreamIdentity,
    revision: u64,
    document: MarkdownDocument,
}

impl ParsedSource {
    fn parse(stream: &MarkdownStream) -> Self {
        Self {
            identity: stream.identity(),
            revision: stream.revision(),
            document: MarkdownDocument::parse(stream.source()),
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
        options: MarkdownRenderOptions,
        theme: &ReviewTheme,
        highlighter: &mut SyntaxHighlighter,
    ) -> Arc<[Line<'static>]> {
        Arc::from(document_rows(document, options, theme, highlighter))
    }

    pub fn render_stream_lines(
        &self,
        state: &mut StreamingMarkdownState,
        stream: &MarkdownStream,
        options: MarkdownRenderOptions,
        theme: &ReviewTheme,
        highlighter: &mut SyntaxHighlighter,
    ) -> Arc<[Line<'static>]> {
        let theme_revision = theme.revision();
        let parsed = match state.parsed.take() {
            Some(parsed) if parsed.matches(stream) => {
                if state.options == options && state.theme_revision == theme_revision {
                    state.stats.rows_reused += state.lines.len();
                    state.parsed = Some(parsed);
                    return Arc::clone(&state.lines);
                }
                parsed
            }
            _ => {
                state.stats.parsed_bytes += stream.source().len();
                state.stats.parsed_documents += 1;
                ParsedSource::parse(stream)
            }
        };
        let rows = document_rows(&parsed.document, options, theme, highlighter);
        state.stats.rows_generated += rows.len();
        state.parsed = Some(parsed);
        state.options = options;
        state.theme_revision = theme_revision;
        state.lines = Arc::from(rows);
        Arc::clone(&state.lines)
    }
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
    highlights: &DocumentHighlights,
    theme: &ReviewTheme,
    width: u16,
    prefix: &str,
) -> Vec<Line<'static>> {
    let base = Style::new()
        .fg(color(theme.markdown.code))
        .bg(color(theme.diff.background));
    let mut output = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let spans = highlights.line(index).unwrap_or_default();
        let mut rendered = highlighted_line(line, spans, base);
        if !prefix.is_empty() {
            rendered
                .spans
                .insert(0, Span::styled(prefix.to_owned(), base));
        }
        push_wrapped_spans(&mut output, rendered.spans, width);
    }
    output
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
            let lines = code
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>();
            let highlights = highlighter
                .with_theme(&theme.syntax)
                .highlight_document_lines(
                    SourceSequenceId::from_lines(lines.iter().copied()),
                    LanguageHint::InfoString(code.highlight_hint()),
                    lines.iter().copied(),
                );
            output.extend(code_rows(&lines, &highlights, theme, width, prefix));
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
        assert!(!stream.is_finished());
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
    fn stream_stats_reset_and_changed_snapshots_parse_complete_context() {
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
        assert_eq!(second.parsed_bytes, stream.source().len());
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
