//! Read-only whole-document and append-stream Markdown rendering.

use crate::syntax::highlighted_line;
use diff_markdown::{
    MarkdownBlock, MarkdownBlockKind, MarkdownDocument, MarkdownStream, rendered_text,
};
use diff_syntax::{LanguageHint, SyntaxHighlighter};
use diff_theme::{Fingerprint, ReviewTheme, Rgba};
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

/// Renderer-owned cache for one logical streaming Markdown item.
#[derive(Debug, Clone, Default)]
pub struct StreamingMarkdownState {
    revision: Option<u64>,
    options: MarkdownRenderOptions,
    theme_revision: Fingerprint,
    stable_offset: usize,
    stable_lines: Arc<[Line<'static>]>,
    fence_opening: Option<usize>,
    fence_prefix_lines: Arc<[Line<'static>]>,
    lines: Arc<[Line<'static>]>,
}

impl StreamingMarkdownState {
    pub fn reset(&mut self) {
        *self = Self::default();
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
        Arc::from(output)
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
            || state.options != options
            || state.theme_revision != theme_revision
            || stable_offset < state.stable_offset;

        if cache_invalid || stable_offset != state.stable_offset {
            let stable_document = MarkdownDocument::parse(&stream.source()[..stable_offset]);
            state.stable_lines = self.render_lines(&stable_document, options, theme, highlighter);
            state.stable_offset = stable_offset;
        }

        let (prefix_lines, tail_lines) = if let Some(continuation) = stream.continuation() {
            // The stable split may fall inside an open fenced block. Parsing the
            // unstable tail alone would turn it into prose and discard multiline
            // syntax context, so retain the pre-fence prefix and render the live
            // fence as one semantic block.
            let opening = continuation.opening_line();
            if cache_invalid || state.fence_opening != Some(opening) {
                let prefix_document = MarkdownDocument::parse(&stream.source()[..opening]);
                state.fence_prefix_lines =
                    self.render_lines(&prefix_document, options, theme, highlighter);
                state.fence_opening = Some(opening);
            }
            let fence_document = MarkdownDocument::parse(&stream.source()[opening..]);
            (
                Arc::clone(&state.fence_prefix_lines),
                self.render_lines(&fence_document, options, theme, highlighter),
            )
        } else {
            state.fence_opening = None;
            state.fence_prefix_lines = Arc::from([]);
            let tail = &stream.source()[stable_offset..];
            let tail_lines = if tail.is_empty() {
                Arc::from([])
            } else {
                let tail_document = MarkdownDocument::parse(tail);
                self.render_lines(&tail_document, options, theme, highlighter)
            };
            (Arc::clone(&state.stable_lines), tail_lines)
        };
        let separator = usize::from(
            options.block_spacing && !prefix_lines.is_empty() && !tail_lines.is_empty(),
        );
        let mut combined = Vec::with_capacity(prefix_lines.len() + separator + tail_lines.len());
        combined.extend(prefix_lines.iter().cloned());
        if separator != 0 {
            combined.push(Line::default());
        }
        combined.extend(tail_lines.iter().cloned());
        let lines = Arc::from(combined);
        state.revision = Some(revision);
        state.options = options;
        state.theme_revision = theme_revision;
        state.lines = Arc::clone(&lines);
        lines
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "semantic block dispatch is clearer as one exhaustive match"
)]
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
            push_wrapped(
                output,
                &format!("{prefix}{marker}{}", rendered_text(content)),
                width,
                Style::new()
                    .fg(color(theme.markdown.heading))
                    .add_modifier(Modifier::BOLD),
            );
        }
        MarkdownBlockKind::Paragraph { content } | MarkdownBlockKind::HtmlFallback { content } => {
            push_wrapped(
                output,
                &format!("{prefix}{}", rendered_text(content)),
                width,
                Style::new().fg(color(theme.diff.foreground)),
            );
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
                push_wrapped(
                    output,
                    &format!(
                        "{prefix}{}{marker} {}",
                        "  ".repeat(item.depth),
                        rendered_text(&item.content)
                    ),
                    width,
                    Style::new().fg(color(theme.diff.foreground)),
                );
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
            let hint = code
                .info
                .as_deref()
                .or(code.language.as_deref())
                .unwrap_or_default();
            let line_spans = highlighter.highlight_sequential(
                &theme.syntax,
                LanguageHint::InfoString(hint),
                source.iter().copied(),
            );
            let base = Style::new()
                .fg(color(theme.markdown.code))
                .bg(color(theme.diff.background));
            for (line, spans) in source.iter().zip(line_spans) {
                let mut rendered = highlighted_line(line, &spans, base);
                if !prefix.is_empty() {
                    rendered
                        .spans
                        .insert(0, Span::styled(prefix.to_owned(), base));
                }
                output.push(rendered);
            }
        }
        MarkdownBlockKind::Table(table) => {
            for row in &table.rows {
                let text = row
                    .cells
                    .iter()
                    .map(|cell| rendered_text(&cell.content))
                    .collect::<Vec<_>>()
                    .join(" │ ");
                push_wrapped(
                    output,
                    &format!("{prefix}│ {text} │"),
                    width,
                    Style::new().fg(color(theme.diff.foreground)),
                );
            }
        }
        MarkdownBlockKind::Rule => output.push(Line::styled(
            "─".repeat(usize::from(width)),
            Style::new().fg(color(theme.diff.border)),
        )),
    }
}

fn push_wrapped(output: &mut Vec<Line<'static>>, text: &str, width: u16, style: Style) {
    let width = usize::from(width.max(1));
    for logical in text.split('\n') {
        let mut row = String::new();
        let mut used = 0;
        for character in logical.chars() {
            let character_width = character.width().unwrap_or(0);
            if used + character_width > width && !row.is_empty() {
                output.push(Line::styled(std::mem::take(&mut row), style));
                used = 0;
            }
            row.push(character);
            used += character_width;
        }
        output.push(Line::styled(row, style));
    }
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
