//! Visible-range Ratatui rendering.

use crate::{DiffReviewState, DiffReviewStatus, FocusPane, RatatuiTheme};
use diff_core::{DiffTone, HighlightSpan, PresentedCell, PresentedRow, RowKind, ViewMode};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, StatefulWidget, Widget},
};
use unicode_width::UnicodeWidthStr;

const DRAWER_BREAKPOINT: u16 = 72;
const FOOTER_HEIGHT: u16 = 1;

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
    }
}

fn render_body(area: Rect, buffer: &mut Buffer, state: &mut DiffReviewState, theme: &RatatuiTheme) {
    match &state.status {
        DiffReviewStatus::Loading => {
            Paragraph::new("Loading diff…")
                .style(Style::new().fg(theme.muted))
                .render(area, buffer);
        }
        DiffReviewStatus::Error(message) => {
            Paragraph::new(format!("Diff unavailable: {message}"))
                .style(Style::new().fg(theme.deletion))
                .render(area, buffer);
        }
        DiffReviewStatus::Ready if state.document.files.is_empty() => {
            Paragraph::new("No changes")
                .style(Style::new().fg(theme.muted))
                .render(area, buffer);
        }
        DiffReviewStatus::Ready => render_document(area, buffer, state, theme),
    }
}

fn render_document(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut DiffReviewState,
    theme: &RatatuiTheme,
) {
    let (drawer, patch) = if area.width >= DRAWER_BREAKPOINT {
        let drawer_width = (area.width / 3).clamp(20, 36);
        let [drawer, separator, patch] = Layout::horizontal([
            Constraint::Length(drawer_width),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(area);
        Paragraph::new("│")
            .style(Style::new().fg(theme.border))
            .render(separator, buffer);
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

fn render_drawer(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut DiffReviewState,
    theme: &RatatuiTheme,
) {
    state.drawer_height = usize::from(area.height).max(1);
    state.drawer_scroll = state.drawer_scroll.min(
        state
            .document
            .files
            .len()
            .saturating_sub(state.drawer_height),
    );
    for (offset, file) in state
        .document
        .files
        .iter()
        .skip(state.drawer_scroll)
        .take(state.drawer_height)
        .enumerate()
    {
        let y = area
            .y
            .saturating_add(u16::try_from(offset).unwrap_or(u16::MAX));
        let selected = state.drawer_scroll.saturating_add(offset) == state.selected_file;
        let marker = match file.staged {
            diff_core::StageState::Unstaged => "☐",
            diff_core::StageState::Staged => "☑",
            diff_core::StageState::PartiallyStaged => "◩",
        };
        let status = match file.status {
            diff_core::FileStatus::Modified => "M",
            diff_core::FileStatus::Added => "A",
            diff_core::FileStatus::Deleted => "D",
            diff_core::FileStatus::Renamed => "R",
            diff_core::FileStatus::Copied => "C",
            diff_core::FileStatus::Untracked => "?",
        };
        let style = if selected {
            Style::new()
                .fg(theme.background)
                .bg(theme.accent)
                .add_modifier(if state.focus == FocusPane::Files {
                    Modifier::BOLD
                } else {
                    Modifier::default()
                })
        } else {
            Style::new().fg(theme.foreground).bg(theme.background)
        };
        buffer.set_style(Rect::new(area.x, y, area.width, 1), style);
        Paragraph::new(format!(
            " {marker} {status} {} +{} -{}",
            file.path,
            file.additions(),
            file.deletions()
        ))
        .style(style)
        .render(Rect::new(area.x, y, area.width, 1), buffer);
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
    let Some(range) = state.presentation.file_range(state.selected_file) else {
        return;
    };
    state.selected_row = state
        .selected_row
        .clamp(range.start, range.end.saturating_sub(1));
    state.last_height = usize::from(area.height).max(1);
    state.follow_selection();
    state.scroll = state.scroll.clamp(range.start, range.end.saturating_sub(1));
    state.visible_rows.clear();

    let mut y = area.y;
    let mut index = state.scroll;
    while y < area.bottom() && index < range.end {
        let Some(row) = state.presentation.row(index).cloned() else {
            break;
        };
        render_row(
            Rect::new(area.x, y, area.width, 1),
            buffer,
            state,
            theme,
            &row,
            index,
        );
        state.visible_rows.push((y, index));
        y = y.saturating_add(1);
        if row.kind == RowKind::Code {
            for comment in comments_for_row(state, &row) {
                if y >= area.bottom() {
                    break;
                }
                let prefix = if comment.outdated {
                    "↳ [outdated] "
                } else {
                    "↳ "
                };
                Paragraph::new(format!("{prefix}{}", comment.body.replace('\n', " ⏎ ")))
                    .style(Style::new().fg(theme.accent).bg(theme.background))
                    .render(Rect::new(area.x, y, area.width, 1), buffer);
                y = y.saturating_add(1);
            }
            if let Some(draft) = state
                .draft
                .as_ref()
                .filter(|draft| row_has_anchor(&row, &draft.anchor))
                && y < area.bottom()
            {
                let before_cursor = draft.text[..draft.cursor].replace('\n', " ⏎ ");
                let cursor_offset = u16::try_from(before_cursor.width()).unwrap_or(u16::MAX);
                state.cursor_position = Some(ratatui::layout::Position::new(
                    area.x
                        .saturating_add(2)
                        .saturating_add(cursor_offset)
                        .min(area.right().saturating_sub(1)),
                    y,
                ));
                Paragraph::new(format!("✎ {}", draft.text.replace('\n', " ⏎ ")))
                    .style(Style::new().fg(theme.accent).bg(theme.background))
                    .render(Rect::new(area.x, y, area.width, 1), buffer);
                y = y.saturating_add(1);
            }
        }
        index += 1;
    }
}

fn comments_for_row<'a>(
    state: &'a DiffReviewState,
    row: &'a PresentedRow,
) -> impl Iterator<Item = &'a diff_core::ReviewComment> {
    state
        .review
        .comments()
        .iter()
        .filter(|comment| row_has_anchor(row, &comment.anchor))
}

fn row_has_anchor(row: &PresentedRow, anchor: &diff_core::LineAnchor) -> bool {
    [row.left.as_ref(), row.right.as_ref()]
        .into_iter()
        .flatten()
        .any(|cell| cell.anchor.as_ref() == Some(anchor))
}

fn render_row(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut DiffReviewState,
    theme: &RatatuiTheme,
    row: &PresentedRow,
    index: usize,
) {
    let selected = index == state.selected_row && state.focus == FocusPane::Diff;
    match row.kind {
        RowKind::FileHeader => {
            let file = &state.document.files[row.file_index];
            let line = Line::from(vec![
                Span::styled(
                    format!(" {} ", file.path),
                    Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("+{} -{}", file.additions(), file.deletions()),
                    Style::new().fg(theme.muted),
                ),
            ]);
            Paragraph::new(line).render(area, buffer);
        }
        RowKind::HunkHeader | RowKind::Meta => {
            let text = row
                .right
                .as_ref()
                .or(row.left.as_ref())
                .map_or("", |cell| cell.text.as_ref());
            Paragraph::new(text)
                .style(Style::new().fg(theme.muted).bg(theme.background))
                .render(area, buffer);
        }
        RowKind::Code if state.presentation.view_mode() == ViewMode::Split => {
            let left_width = area.width.saturating_sub(1) / 2;
            let right_width = area.width.saturating_sub(left_width + 1);
            let [left, separator, right] = Layout::horizontal([
                Constraint::Length(left_width),
                Constraint::Length(1),
                Constraint::Length(right_width),
            ])
            .areas(area);
            render_cell(left, buffer, state, theme, row.left.as_ref(), selected);
            Paragraph::new("│")
                .style(Style::new().fg(theme.border).bg(theme.background))
                .render(separator, buffer);
            render_cell(right, buffer, state, theme, row.right.as_ref(), selected);
        }
        RowKind::Code => {
            render_cell(
                area,
                buffer,
                state,
                theme,
                row.right.as_ref().or(row.left.as_ref()),
                selected,
            );
        }
    }
}

fn render_cell(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut DiffReviewState,
    theme: &RatatuiTheme,
    cell: Option<&PresentedCell>,
    selected: bool,
) {
    let tone = cell.map_or(DiffTone::Context, |cell| cell.tone);
    let (foreground, background) = match tone {
        DiffTone::Added => (theme.addition, theme.addition_background),
        DiffTone::Removed => (theme.deletion, theme.deletion_background),
        DiffTone::Context | DiffTone::Meta => (theme.foreground, theme.background),
    };
    let background = if selected {
        theme.selection
    } else {
        background
    };
    buffer.set_style(area, Style::new().fg(foreground).bg(background));
    let Some(cell) = cell else {
        return;
    };
    let number = cell
        .line_number
        .map_or_else(String::new, |number| number.to_string());
    let marker = match cell.tone {
        DiffTone::Added => '+',
        DiffTone::Removed => '-',
        DiffTone::Context | DiffTone::Meta => ' ',
    };
    let gutter = format!("{number:>4} {marker}");
    let language = state.document.files[cell
        .anchor
        .as_ref()
        .and_then(|anchor| {
            state
                .document
                .files
                .iter()
                .position(|file| file.path == anchor.path)
        })
        .unwrap_or(state.selected_file)]
    .language();
    let highlights = state
        .highlighter
        .highlight_cached(&state.theme, language, &cell.text);
    let mut spans = vec![Span::styled(
        gutter,
        Style::new().fg(theme.gutter).bg(background),
    )];
    spans.extend(highlighted_spans(&cell.text, &highlights, background));
    Paragraph::new(Line::from(spans)).render(area, buffer);
}

fn highlighted_spans(
    source: &str,
    highlights: &[HighlightSpan],
    background: ratatui::style::Color,
) -> Vec<Span<'static>> {
    if highlights.is_empty() {
        return vec![Span::styled(source.to_owned(), Style::new().bg(background))];
    }
    let mut spans = Vec::new();
    let mut offset = 0;
    for highlight in highlights {
        let start = highlight.range.start.min(source.len());
        let end = highlight.range.end.min(source.len());
        if start > offset && source.is_char_boundary(offset) && source.is_char_boundary(start) {
            spans.push(Span::styled(
                source[offset..start].to_owned(),
                Style::new().bg(background),
            ));
        }
        if end > start && source.is_char_boundary(start) && source.is_char_boundary(end) {
            spans.push(Span::styled(
                source[start..end].to_owned(),
                crate::style::syntax_style(highlight.foreground, highlight.font_style, background),
            ));
            offset = end;
        }
    }
    if offset < source.len() && source.is_char_boundary(offset) {
        spans.push(Span::styled(
            source[offset..].to_owned(),
            Style::new().bg(background),
        ));
    }
    spans
}

fn render_footer(area: Rect, buffer: &mut Buffer, state: &DiffReviewState, theme: &RatatuiTheme) {
    if area.is_empty() {
        return;
    }
    let text = if state.draft.is_some() {
        "[Enter] save  [Shift-Enter] newline  [Esc] cancel"
    } else if state.focus == FocusPane::Files {
        "[j/k] file  [Enter/l] diff  [v] view  [?] help"
    } else {
        "[j/k] line  [c] comment  [e/x] edit/delete  [s] submit  [y] copy  [h] files"
    };
    let outdated = state
        .review
        .comments()
        .iter()
        .filter(|comment| comment.outdated)
        .count();
    let status = format!(
        "  {} comment{}{}",
        state.review.len(),
        if state.review.len() == 1 { "" } else { "s" },
        if outdated == 0 {
            String::new()
        } else {
            format!(" ({outdated} outdated)")
        }
    );
    Paragraph::new(Line::from(vec![
        Span::styled(text, Style::new().fg(theme.muted)),
        Span::styled(status, Style::new().fg(theme.accent)),
    ]))
    .render(area, buffer);
}

fn render_help(area: Rect, buffer: &mut Buffer, theme: &RatatuiTheme) {
    let width = area.width.saturating_sub(4).min(58);
    let height = area.height.saturating_sub(4).min(12);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    Clear.render(popup, buffer);
    Paragraph::new(
        "Navigation\n  j/k or arrows   move selection\n  h/l or Tab      change pane\n  PgUp/PgDn       move a page\n\nReview\n  c/e/x            add/edit/delete comment\n  s/y              submit/copy review\n  v                cycle layout\n  Esc              cancel or close\n  ?                close help",
    )
    .block(Block::bordered().title(" Review shortcuts "))
    .style(Style::new().fg(theme.foreground).bg(theme.background))
    .render(popup, buffer);
}
