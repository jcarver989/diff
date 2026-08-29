use super::{
    layout::{MarkdownSpan, MarkdownTextStyle, MarkdownVisualLayout},
    state::{MarkdownFocusPane, MarkdownReviewState},
};
use crate::{
    RatatuiTheme,
    annotation::render_annotation_line,
    style::syntax_style,
    widgets::{render_vertical_scrollbar, rows_and_track},
};
use diff_core::HighlightSpan;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, StatefulWidget, Widget},
};

const FOOTER_HEIGHT: u16 = 1;
const GUTTER_SEPARATOR_WIDTH: u16 = 3;
const OUTLINE_WIDTH: u16 = 28;
const OUTLINE_BREAKPOINT: u16 = 90;

/// Stateful Ratatui Markdown review widget.
#[derive(Debug, Clone)]
pub struct MarkdownReviewWidget {
    title: String,
    borders: bool,
}

impl Default for MarkdownReviewWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownReviewWidget {
    /// Creates a bordered widget titled “Markdown Review”.
    #[must_use]
    pub fn new() -> Self {
        Self {
            title: "Markdown Review".to_owned(),
            borders: true,
        }
    }

    /// Sets the outer title.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Enables or disables the outer border.
    #[must_use]
    pub const fn borders(mut self, borders: bool) -> Self {
        self.borders = borders;
        self
    }
}

impl StatefulWidget for MarkdownReviewWidget {
    type State = MarkdownReviewState;

    fn render(self, area: Rect, buffer: &mut Buffer, state: &mut Self::State) {
        state.set_cursor(None);
        let theme = RatatuiTheme::from(&state.theme);
        buffer.set_style(area, Style::new().fg(theme.foreground).bg(theme.background));
        let inner = if self.borders {
            let block = Block::new()
                .borders(Borders::ALL)
                .title(format!(" {} ", self.title))
                .border_style(Style::new().fg(theme.accent));
            let inner = block.inner(area);
            block.render(area, buffer);
            inner
        } else {
            area
        };
        if inner.is_empty() {
            state.dirty = false;
            return;
        }
        let [body, footer] = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(FOOTER_HEIGHT.min(inner.height)),
        ])
        .areas(inner);
        render_body(body, buffer, state, &theme);
        render_footer(footer, buffer, state, &theme);
        if state.help {
            render_help(area, buffer, &theme);
        }
        state.dirty = false;
    }
}

fn render_body(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut MarkdownReviewState,
    theme: &RatatuiTheme,
) {
    state.clear_hit_regions();
    if area.is_empty() {
        return;
    }
    let wide = area.width >= OUTLINE_BREAKPOINT && !state.document().outline().is_empty();
    let (outline, separator, document) = if wide {
        let [outline, separator, document] = Layout::horizontal([
            Constraint::Length(OUTLINE_WIDTH.min(area.width / 3)),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(area);
        (outline, separator, document)
    } else {
        (Rect::default(), Rect::default(), area)
    };
    if wide {
        buffer.set_style(
            separator,
            Style::new().fg(theme.border).bg(theme.background),
        );
        render_outline(outline, buffer, state, theme);
    }
    let (rows, track) = rows_and_track(document, true);
    state.last_height = usize::from(rows.height).max(1);
    let gutter_width = source_gutter_width(state);
    let content_width = rows.width.saturating_sub(gutter_width).max(1);
    let layout = state.ensure_layout(content_width);
    state.follow_selection(&layout);
    let last = layout.rows.len().saturating_sub(state.last_height);
    state.scroll = state.scroll.min(last);
    if layout.rows.is_empty() {
        Paragraph::new("No Markdown content to review")
            .style(Style::new().fg(theme.muted))
            .render(rows, buffer);
    } else {
        render_rows(rows, track, buffer, state, theme, &layout, gutter_width);
    }
}

fn source_gutter_width(state: &MarkdownReviewState) -> u16 {
    let line_count = state.document().source().split('\n').count().max(1);
    let digits = line_count.checked_ilog10().unwrap_or(0).saturating_add(1);
    u16::try_from(digits)
        .unwrap_or(u16::MAX)
        .saturating_add(GUTTER_SEPARATOR_WIDTH)
}

fn render_outline(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut MarkdownReviewState,
    theme: &RatatuiTheme,
) {
    if area.is_empty() {
        return;
    }
    let heading_count = state.document().outline().len();
    state.outline_selected = state.outline_selected.min(heading_count.saturating_sub(1));
    let height = usize::from(area.height);
    let max_scroll = heading_count.saturating_sub(height.max(1));
    state.outline_scroll = state.outline_scroll.min(max_scroll);
    let headings = state.document().outline().to_vec();
    for (offset, heading) in headings
        .iter()
        .skip(state.outline_scroll)
        .take(height)
        .enumerate()
    {
        let row = Rect::new(
            area.x,
            area.y + u16::try_from(offset).unwrap_or(u16::MAX),
            area.width,
            1,
        );
        let index = state.outline_scroll + offset;
        let selected = state.focus == MarkdownFocusPane::Outline && index == state.outline_selected;
        buffer.set_style(
            row,
            Style::new().fg(theme.foreground).bg(if selected {
                theme.selection
            } else {
                theme.background
            }),
        );
        let indent = "  ".repeat(usize::from(heading.level.saturating_sub(1)));
        Paragraph::new(format!("{indent}{}", heading.title))
            .style(Style::new().fg(if selected {
                theme.accent
            } else {
                theme.foreground
            }))
            .render(row, buffer);
        state.hit_regions.push(super::state::MarkdownHitRegion {
            area: row,
            target: Some(heading.target_id),
            outline: true,
        });
    }
}

fn render_rows(
    area: Rect,
    track: Rect,
    buffer: &mut Buffer,
    state: &mut MarkdownReviewState,
    theme: &RatatuiTheme,
    layout: &MarkdownVisualLayout,
    gutter_width: u16,
) {
    let selected = state.selected_target();
    let focused = state.focus == MarkdownFocusPane::Document;
    for (drawn, index) in (state.scroll..layout.rows.len()).enumerate() {
        let y = area
            .y
            .saturating_add(u16::try_from(drawn).unwrap_or(u16::MAX));
        if y >= area.bottom() {
            break;
        }
        let row_area = Rect::new(area.x, y, area.width, 1);
        let row_gutter_width = gutter_width.min(row_area.width.saturating_sub(1));
        let [gutter_area, content_area] =
            Layout::horizontal([Constraint::Length(row_gutter_width), Constraint::Min(1)])
                .areas(row_area);
        let row = &layout.rows[index];
        if let Some((annotation, line)) = &row.annotation {
            render_annotation_line(content_area, buffer, theme, annotation, *line);
            if let Some(column) = annotation.cursor_column(*line) {
                state.set_cursor(Some(ratatui::layout::Position::new(
                    content_area
                        .x
                        .saturating_add(column)
                        .min(content_area.right().saturating_sub(1)),
                    y,
                )));
            }
            continue;
        }
        let is_selected = focused && row.target.is_some() && row.target == selected;
        let background = if is_selected {
            theme.selection
        } else {
            theme.background
        };
        buffer.set_style(row_area, Style::new().fg(theme.foreground).bg(background));
        let gutter = row.source_line.map_or_else(
            || " ".repeat(usize::from(row_gutter_width)),
            |line| {
                let number_width =
                    usize::from(row_gutter_width.saturating_sub(GUTTER_SEPARATOR_WIDTH));
                format!("{line:>number_width$} │ ")
            },
        );
        Paragraph::new(gutter)
            .style(Style::new().fg(theme.gutter).bg(background))
            .render(gutter_area, buffer);
        let mut spans = Vec::new();
        spans.push(Span::styled(
            row.prefix.clone(),
            Style::new()
                .fg(if row.code.is_some() {
                    theme.gutter
                } else {
                    theme.muted
                })
                .bg(background),
        ));
        if let Some(code) = &row.code {
            let source = row
                .spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>();
            let language = code.language.as_deref().unwrap_or("text");
            let highlights = state.highlighter.highlight(&state.theme, language, &source);
            spans.extend(highlighted_spans(&source, &highlights, background));
        } else {
            spans.extend(
                row.spans
                    .iter()
                    .map(|span| styled_span(span, theme, background)),
            );
        }
        Paragraph::new(Line::from(spans)).render(content_area, buffer);
        if row.selectable {
            state.hit_regions.push(super::state::MarkdownHitRegion {
                area: row_area,
                target: row.target,
                outline: false,
            });
        }
    }
    render_vertical_scrollbar(
        track,
        buffer,
        layout.rows.len(),
        usize::from(area.height),
        state.scroll,
    );
}

fn styled_span(
    span: &MarkdownSpan,
    theme: &RatatuiTheme,
    background: ratatui::style::Color,
) -> Span<'static> {
    let mut style = Style::new()
        .fg(match span.style {
            MarkdownTextStyle::Heading | MarkdownTextStyle::Link | MarkdownTextStyle::Image => {
                theme.accent
            }
            MarkdownTextStyle::Muted => theme.muted,
            _ => theme.foreground,
        })
        .bg(background);
    match span.style {
        MarkdownTextStyle::Heading | MarkdownTextStyle::Strong => {
            style = style.add_modifier(Modifier::BOLD);
        }
        MarkdownTextStyle::Emphasis => style = style.add_modifier(Modifier::ITALIC),
        MarkdownTextStyle::Strikethrough => style = style.add_modifier(Modifier::CROSSED_OUT),
        MarkdownTextStyle::Link => style = style.add_modifier(Modifier::UNDERLINED),
        MarkdownTextStyle::InlineCode => style = style.bg(theme.border),
        _ => {}
    }
    Span::styled(span.text.clone(), style)
}

fn highlighted_spans(
    source: &str,
    highlights: &[HighlightSpan],
    background: ratatui::style::Color,
) -> Vec<Span<'static>> {
    if highlights.is_empty() {
        return vec![Span::styled(source.to_owned(), Style::new().bg(background))];
    }
    let mut output = Vec::new();
    let mut offset = 0;
    for highlight in highlights {
        let start = highlight.range.start.min(source.len());
        let end = highlight.range.end.min(source.len());
        if start > offset && source.is_char_boundary(start) {
            output.push(Span::styled(
                source[offset..start].to_owned(),
                Style::new().bg(background),
            ));
        }
        if end > start && source.is_char_boundary(start) && source.is_char_boundary(end) {
            output.push(Span::styled(
                source[start..end].to_owned(),
                syntax_style(highlight.foreground, highlight.font_style, background),
            ));
            offset = end;
        }
    }
    if offset < source.len() && source.is_char_boundary(offset) {
        output.push(Span::styled(
            source[offset..].to_owned(),
            Style::new().bg(background),
        ));
    }
    output
}

fn render_footer(
    area: Rect,
    buffer: &mut Buffer,
    state: &MarkdownReviewState,
    theme: &RatatuiTheme,
) {
    if area.is_empty() {
        return;
    }
    let hint = if state.session.draft().is_some() {
        "[Enter] save  [Shift-Enter] newline  [Esc] cancel"
    } else {
        "[j/k] target  [n/p] heading  [c] comment  [a] approve  [r] request changes  [?] help"
    };
    let count = state.review().len();
    Paragraph::new(Line::from(vec![
        Span::styled(hint, Style::new().fg(theme.muted)),
        Span::styled(
            format!("  {count} comment{}", if count == 1 { "" } else { "s" }),
            Style::new().fg(theme.accent),
        ),
    ]))
    .render(area, buffer);
}

fn render_help(area: Rect, buffer: &mut Buffer, theme: &RatatuiTheme) {
    let width = area.width.saturating_sub(4).min(72);
    let height = area.height.saturating_sub(4).min(12);
    if width == 0 || height == 0 {
        return;
    }
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    Clear.render(popup, buffer);
    Paragraph::new("Navigation\n  j/k or arrows   move target\n  g/G or Home/End first/last\n  n/p             next/previous heading\n  h/l or Enter    outline/document\n\nReview\n  c/e/x/u         add/edit/delete/undo\n  a/r             approve/request changes\n  Esc             cancel draft/review\n  ?               close help")
        .block(Block::bordered().title(" Markdown shortcuts "))
        .style(Style::new().fg(theme.foreground).bg(theme.background))
        .render(popup, buffer);
}
