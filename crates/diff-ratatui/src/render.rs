//! Visible-range Ratatui rendering.

use crate::{DiffReviewState, DiffReviewStatus, FocusPane, RatatuiTheme, style::syntax_style};
use diff_core::{
    CommentDraft, DiffTheme, DiffTone, HighlightSpan, PresentedCell, PresentedRow, ReviewComment,
    RowKind, SyntaxHighlighter,
};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, StatefulWidget, Widget},
};
use unicode_width::UnicodeWidthStr;

const DRAWER_BREAKPOINT: u16 = 72;
const DRAWER_MIN_WIDTH: u16 = 20;
const DRAWER_MAX_WIDTH: u16 = 36;
const FOOTER_HEIGHT: u16 = 1;
const GUTTER_WIDTH: u16 = 6;

/// Embeddable stateful diff review widget.
#[derive(Debug, Clone)]
pub struct DiffReviewWidget {
    title: String,
    borders: bool,
}

impl Default for DiffReviewWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl DiffReviewWidget {
    /// Creates a bordered widget titled “Diff Review”.
    #[must_use]
    pub fn new() -> Self {
        Self {
            title: "Diff Review".to_owned(),
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

impl StatefulWidget for DiffReviewWidget {
    type State = DiffReviewState;

    fn render(self, area: Rect, buffer: &mut Buffer, state: &mut Self::State) {
        state.cursor_position = None;
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

fn render_body(area: Rect, buffer: &mut Buffer, state: &mut DiffReviewState, theme: &RatatuiTheme) {
    let notice = match &state.status {
        DiffReviewStatus::Loading => Some(("Loading diff…".to_owned(), theme.muted)),
        DiffReviewStatus::Error(message) => {
            Some((format!("Diff unavailable: {message}"), theme.deletion))
        }
        DiffReviewStatus::Ready if state.document().files.is_empty() => {
            Some(("No changes".to_owned(), theme.muted))
        }
        DiffReviewStatus::Ready => None,
    };
    match notice {
        Some((text, foreground)) => Paragraph::new(text)
            .style(Style::new().fg(foreground))
            .render(area, buffer),
        None => render_document(area, buffer, state, theme),
    }
}

fn render_document(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut DiffReviewState,
    theme: &RatatuiTheme,
) {
    let (drawer, patch) = if area.width >= DRAWER_BREAKPOINT {
        let drawer_width = (area.width / 3).clamp(DRAWER_MIN_WIDTH, DRAWER_MAX_WIDTH);
        let [drawer, separator, patch] = Layout::horizontal([
            Constraint::Length(drawer_width),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(area);
        render_separator(separator, buffer, theme);
        render_drawer(drawer, buffer, state, theme);
        (drawer, patch)
    } else {
        (Rect::default(), area)
    };
    state.hit_layout.drawer = drawer;
    state.hit_layout.patch = patch;
    state.ensure_presentation(patch.width);
    render_patch(patch, buffer, state, theme);
}

fn render_separator(area: Rect, buffer: &mut Buffer, theme: &RatatuiTheme) {
    Paragraph::new("│")
        .style(Style::new().fg(theme.border).bg(theme.background))
        .render(area, buffer);
}

fn render_drawer(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut DiffReviewState,
    theme: &RatatuiTheme,
) {
    state.drawer_height = usize::from(area.height).max(1);
    let file_count = state.document().files.len();
    state.drawer_scroll = state
        .drawer_scroll
        .min(file_count.saturating_sub(state.drawer_height));
    let selected_file = state.selected_file();
    let DiffReviewState {
        session,
        drawer_scroll,
        drawer_height,
        focus,
        ..
    } = state;
    for (offset, file) in session
        .document()
        .files
        .iter()
        .skip(*drawer_scroll)
        .take(*drawer_height)
        .enumerate()
    {
        let y = area
            .y
            .saturating_add(u16::try_from(offset).unwrap_or(u16::MAX));
        let index = drawer_scroll.saturating_add(offset);
        let style = if selected_file == Some(index) {
            Style::new()
                .fg(theme.background)
                .bg(theme.accent)
                .add_modifier(if *focus == FocusPane::Files {
                    Modifier::BOLD
                } else {
                    Modifier::default()
                })
        } else {
            Style::new().fg(theme.foreground).bg(theme.background)
        };
        let row = Rect::new(area.x, y, area.width, 1);
        buffer.set_style(row, style);
        Paragraph::new(format!(
            " {} {} {} +{} -{}",
            stage_marker(file.staged),
            file.status.code(),
            file.path,
            file.additions(),
            file.deletions()
        ))
        .style(style)
        .render(row, buffer);
    }
}

const fn stage_marker(state: diff_core::StageState) -> &'static str {
    match state {
        diff_core::StageState::Unstaged => "☐",
        diff_core::StageState::Staged => "☑",
        diff_core::StageState::PartiallyStaged => "◩",
    }
}

fn render_patch(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut DiffReviewState,
    theme: &RatatuiTheme,
) {
    if area.is_empty() {
        return;
    }
    let Some(range) = state.session.selected_file_range() else {
        return;
    };
    state.last_height = usize::from(area.height).max(1);
    state.highlighter.reserve_for_viewport(state.last_height);
    if state.take_follow_request() {
        state.follow_selection();
    }
    state.scroll = state.scroll.clamp(range.start, range.end.saturating_sub(1));

    let DiffReviewState {
        session,
        theme: diff_theme,
        highlighter,
        focus,
        scroll,
        visible_rows,
        cursor_position,
        ..
    } = state;
    visible_rows.clear();
    let presentation = session.presentation();
    let selected_row = session.selected_row();
    let selected_side = session.selected_side();
    let draft = session.draft();
    let comments = session.review().comments();
    let layout = session.layout();

    let mut y = area.y;
    let mut index = *scroll;
    while y < area.bottom() && index < range.end {
        let Some(row) = presentation.row(index) else {
            break;
        };
        let mut context = CellContext {
            theme,
            diff_theme,
            highlighter,
            presentation,
            row,
        };
        let selected = selected_row == Some(index) && *focus == FocusPane::Diff;
        render_row(
            Rect::new(area.x, y, area.width, 1),
            buffer,
            &mut context,
            row,
            &RowStyle {
                selected,
                selected_side,
                layout,
                file_stats: file_stats(session, row),
            },
        );
        visible_rows.push((y, index));
        y = y.saturating_add(1);

        if row.kind == RowKind::Code {
            for comment in comments
                .iter()
                .filter(|comment| presentation.row_shows_anchor(row, &comment.anchor))
            {
                if y >= area.bottom() {
                    break;
                }
                render_annotation(Rect::new(area.x, y, area.width, 1), buffer, theme, comment);
                y = y.saturating_add(1);
            }
            if let Some(draft) =
                draft.filter(|draft| presentation.row_shows_anchor(row, draft.anchor()))
                && y < area.bottom()
            {
                *cursor_position = Some(render_draft(
                    Rect::new(area.x, y, area.width, 1),
                    buffer,
                    theme,
                    draft,
                ));
                y = y.saturating_add(1);
            }
        }
        index += 1;
    }
}

fn file_stats(session: &diff_core::ReviewSession, row: &PresentedRow) -> Option<(usize, usize)> {
    (row.kind == RowKind::FileHeader)
        .then(|| session.document().files.get(row.file_index))
        .flatten()
        .map(|file| (file.additions(), file.deletions()))
}

struct RowStyle {
    selected: bool,
    selected_side: diff_core::DiffSide,
    layout: diff_core::Layout,
    file_stats: Option<(usize, usize)>,
}

struct CellContext<'a> {
    theme: &'a RatatuiTheme,
    diff_theme: &'a DiffTheme,
    highlighter: &'a mut SyntaxHighlighter,
    presentation: &'a diff_core::DiffPresentation,
    row: &'a PresentedRow,
}

fn render_annotation(
    area: Rect,
    buffer: &mut Buffer,
    theme: &RatatuiTheme,
    comment: &ReviewComment,
) {
    let prefix = if comment.outdated {
        "↳ [outdated] "
    } else {
        "↳ "
    };
    Paragraph::new(format!("{prefix}{}", comment.body.replace('\n', " ⏎ ")))
        .style(Style::new().fg(theme.accent).bg(theme.background))
        .render(area, buffer);
}

fn render_draft(
    area: Rect,
    buffer: &mut Buffer,
    theme: &RatatuiTheme,
    draft: &CommentDraft,
) -> Position {
    let before_cursor = draft.body_before_cursor().replace('\n', " ⏎ ");
    let cursor_offset = u16::try_from(before_cursor.width()).unwrap_or(u16::MAX);
    Paragraph::new(format!("✎ {}", draft.body().replace('\n', " ⏎ ")))
        .style(Style::new().fg(theme.accent).bg(theme.background))
        .render(area, buffer);
    Position::new(
        area.x
            .saturating_add(2)
            .saturating_add(cursor_offset)
            .min(area.right().saturating_sub(1)),
        area.y,
    )
}

fn render_row(
    area: Rect,
    buffer: &mut Buffer,
    context: &mut CellContext<'_>,
    row: &PresentedRow,
    style: &RowStyle,
) {
    match row.kind {
        RowKind::FileHeader => {
            let text = row.primary_cell().map_or("", |cell| cell.text.as_ref());
            let (additions, deletions) = style.file_stats.unwrap_or_default();
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" {text} "),
                    Style::new()
                        .fg(context.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("+{additions} -{deletions}"),
                    Style::new().fg(context.theme.muted),
                ),
            ]))
            .render(area, buffer);
        }
        RowKind::HunkHeader | RowKind::Meta => {
            Paragraph::new(row.primary_cell().map_or("", |cell| cell.text.as_ref()))
                .style(
                    Style::new()
                        .fg(context.theme.muted)
                        .bg(context.theme.background),
                )
                .render(area, buffer);
        }
        RowKind::Code if style.layout.is_split() => {
            let left_width = area.width.saturating_sub(1) / 2;
            let right_width = area.width.saturating_sub(left_width + 1);
            let [left, separator, right] = Layout::horizontal([
                Constraint::Length(left_width),
                Constraint::Length(1),
                Constraint::Length(right_width),
            ])
            .areas(area);
            let focused = |side| style.selected && style.selected_side == side;
            render_cell(
                left,
                buffer,
                context,
                row.left.as_ref(),
                focused(diff_core::DiffSide::Old),
            );
            render_separator(separator, buffer, context.theme);
            render_cell(
                right,
                buffer,
                context,
                row.right.as_ref(),
                focused(diff_core::DiffSide::New),
            );
        }
        RowKind::Code => {
            render_cell(area, buffer, context, row.primary_cell(), style.selected);
        }
    }
}

fn render_cell(
    area: Rect,
    buffer: &mut Buffer,
    context: &mut CellContext<'_>,
    cell: Option<&PresentedCell>,
    selected: bool,
) {
    let tone = cell.map_or(DiffTone::Context, |cell| cell.tone);
    let (foreground, tone_background) = context.theme.tone(tone);
    let background = if selected {
        context.theme.selection
    } else {
        tone_background
    };
    buffer.set_style(area, Style::new().fg(foreground).bg(background));
    let Some(cell) = cell else {
        return;
    };
    let number = cell
        .line_number
        .map_or_else(String::new, |number| number.to_string());
    let gutter = format!(
        "{number:>width$} {marker}",
        width = usize::from(GUTTER_WIDTH) - 2,
        marker = tone.marker()
    );
    let highlights = context.presentation.highlight_cell(
        context.highlighter,
        context.diff_theme,
        context.row,
        cell,
    );
    let mut spans = vec![Span::styled(
        gutter,
        Style::new().fg(context.theme.gutter).bg(background),
    )];
    spans.extend(highlighted_spans(&cell.text, &highlights, background));
    Paragraph::new(Line::from(spans)).render(area, buffer);
}

fn highlighted_spans<'a>(
    source: &'a str,
    highlights: &[HighlightSpan],
    background: ratatui::style::Color,
) -> Vec<Span<'a>> {
    if highlights.is_empty() {
        return vec![Span::styled(source, Style::new().bg(background))];
    }
    let plain = Style::new().bg(background);
    let mut spans = Vec::new();
    let mut offset = 0;
    for highlight in highlights {
        let start = highlight.range.start.min(source.len());
        let end = highlight.range.end.min(source.len());
        if start > offset && source.is_char_boundary(offset) && source.is_char_boundary(start) {
            spans.push(Span::styled(&source[offset..start], plain));
        }
        if end > start && source.is_char_boundary(start) && source.is_char_boundary(end) {
            spans.push(Span::styled(
                &source[start..end],
                syntax_style(highlight.foreground, highlight.font_style, background),
            ));
            offset = end;
        }
    }
    if offset < source.len() && source.is_char_boundary(offset) {
        spans.push(Span::styled(&source[offset..], plain));
    }
    spans
}

fn render_footer(area: Rect, buffer: &mut Buffer, state: &DiffReviewState, theme: &RatatuiTheme) {
    if area.is_empty() {
        return;
    }
    let hint = if state.session.draft().is_some() {
        "[Enter] save  [Shift-Enter] newline  [Esc] cancel"
    } else if state.focus == FocusPane::Files {
        "[j/k] file  [Enter/l] diff  [v] view  [?] help"
    } else if state.layout().is_split() {
        "[j/k] line  [←/→] side  [c] comment  [s] submit  [h] files  [?] help"
    } else {
        "[j/k] line  [c] comment  [e/x] edit/delete  [s] submit  [y] copy  [h] files"
    };
    let review = state.review();
    let outdated = review.outdated_count();
    let status = format!(
        "  {} comment{}{}",
        review.len(),
        if review.len() == 1 { "" } else { "s" },
        if outdated == 0 {
            String::new()
        } else {
            format!(" ({outdated} outdated)")
        }
    );
    Paragraph::new(Line::from(vec![
        Span::styled(hint, Style::new().fg(theme.muted)),
        Span::styled(status, Style::new().fg(theme.accent)),
    ]))
    .render(area, buffer);
}

fn render_help(area: Rect, buffer: &mut Buffer, theme: &RatatuiTheme) {
    let width = area.width.saturating_sub(4).min(58);
    let height = area.height.saturating_sub(4).min(13);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    Clear.render(popup, buffer);
    Paragraph::new(
        "Navigation\n  j/k or arrows   move selection\n  h/l or Tab      change pane\n  ←/→ in split    change column\n  PgUp/PgDn       move a page\n\nReview\n  c/e/x            add/edit/delete comment\n  s/y              submit/copy review\n  v                cycle layout\n  Esc              cancel or close\n  ?                close help",
    )
    .block(Block::bordered().title(" Review shortcuts "))
    .style(Style::new().fg(theme.foreground).bg(theme.background))
    .render(popup, buffer);
}
