//! Visible-range Ratatui rendering.

use crate::{
    DiffReviewState, DiffReviewStatus, FocusPane, RatatuiTheme, RepositoryOperationStatus,
    annotation::render_annotation_line,
    drawer::{DrawerEntry, DrawerTree},
    patch_layout::PatchVisualRow,
    state::RepositoryPrompt,
    style::syntax_style,
    theme_picker::render_theme_picker,
    ui::{ActionBar, AppFrame, EmptyState, Modal, NoticeTone, render_modal_text},
    widgets::{render_vertical_scrollbar, rows_and_track},
};
use diff_core::{DiffTone, PresentedCell, PresentedRow, RowKind};
use diff_syntax::{HighlightSpan, SyntaxHighlighter};
use diff_theme::DiffTheme;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, StatefulWidget, Widget},
};

const DRAWER_BREAKPOINT: u16 = 72;
const DRAWER_MIN_WIDTH: u16 = 20;
const DRAWER_MAX_WIDTH: u16 = 36;
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
        let regions = AppFrame::new(&self.title, self.borders, &theme).render(area, buffer);
        let body = regions.body;
        let footer = regions.footer;
        render_body(body, buffer, state, &theme);
        render_footer(footer, buffer, state, &theme);
        if state.help {
            render_help(area, buffer, &theme);
        }
        if let Some(picker) = &state.theme_picker {
            render_theme_picker(area, buffer, picker, &theme);
        }
        state.dirty = false;
    }
}

fn render_body(area: Rect, buffer: &mut Buffer, state: &mut DiffReviewState, theme: &RatatuiTheme) {
    let notice = match &state.status {
        DiffReviewStatus::Loading => Some(("Loading diff…".to_owned(), NoticeTone::Info)),
        DiffReviewStatus::Error(message) => {
            Some((format!("Diff unavailable: {message}"), NoticeTone::Error))
        }
        DiffReviewStatus::Ready if state.document().files.is_empty() => {
            Some(("No changes".to_owned(), NoticeTone::Info))
        }
        DiffReviewStatus::Ready => None,
    };
    match notice {
        Some((text, tone)) => EmptyState::new(&text, tone, theme).render(area, buffer),
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
        let drawer_hit = render_drawer(drawer, buffer, state, theme);
        (drawer_hit, patch)
    } else {
        (Rect::default(), area)
    };
    state.hit_layout.drawer = drawer;
    if drawer.is_empty() {
        state.hit_layout.drawer_stage_column = None;
    }
    state.hit_layout.patch = patch;
    let (patch_rows, patch_track) = rows_and_track(patch, true);
    state.ensure_presentation(patch_rows.width);
    render_patch(patch_rows, patch_track, buffer, state, theme);
}

fn render_separator(area: Rect, buffer: &mut Buffer, theme: &RatatuiTheme) {
    Paragraph::new("│")
        .style(Style::new().fg(theme.ui.border).bg(theme.ui.canvas))
        .render(area, buffer);
}

#[expect(clippy::too_many_lines, reason = "tree row rendering is kept together")]
fn render_drawer(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut DiffReviewState,
    theme: &RatatuiTheme,
) -> Rect {
    let (rows, track) = rows_and_track(area, true);
    state.hit_layout.drawer_stage_column = rows
        .width
        .checked_sub(1)
        .map(|offset| rows.x.saturating_add(offset));
    state.drawer_height = usize::from(rows.height).max(1);
    let entry_count = state.drawer.entries().len();
    state.drawer_scroll = state
        .drawer_scroll
        .min(entry_count.saturating_sub(state.drawer_height));
    if state.take_drawer_follow_request() {
        if state.drawer_selected < state.drawer_scroll {
            state.drawer_scroll = state.drawer_selected;
        } else if state.drawer_selected >= state.drawer_scroll.saturating_add(state.drawer_height) {
            state.drawer_scroll = state
                .drawer_selected
                .saturating_sub(state.drawer_height.saturating_sub(1));
        }
    }
    for (offset, entry) in state
        .drawer
        .entries()
        .iter()
        .skip(state.drawer_scroll)
        .take(state.drawer_height)
        .enumerate()
    {
        let y = rows
            .y
            .saturating_add(u16::try_from(offset).unwrap_or(u16::MAX));
        let index = state.drawer_scroll.saturating_add(offset);
        let row = Rect::new(rows.x, y, rows.width, 1);
        let [content, stage_area] =
            Layout::horizontal([Constraint::Min(0), Constraint::Length(2_u16.min(row.width))])
                .areas(row);
        let checkbox = Rect::new(
            stage_area
                .x
                .saturating_add(stage_area.width.saturating_sub(1)),
            y,
            stage_area.width.min(1),
            1,
        );
        let entry_stage = match entry {
            DrawerEntry::Directory {
                name,
                depth,
                expanded,
                ..
            } => {
                let marker = if *expanded { "▾" } else { "▸" };
                Paragraph::new(Line::from(vec![
                    Span::raw(format!("{}{} ", "  ".repeat(*depth), marker)),
                    Span::styled(format!("{name}/"), Style::new().fg(theme.ui.accent)),
                ]))
                .render(content, buffer);
                DrawerTree::stage_state_for_entry(state.document(), entry)
            }
            DrawerEntry::File {
                index: file_index,
                name,
                depth,
            } => {
                let Some(file) = state.document().files.get(*file_index) else {
                    continue;
                };
                let status_color = match file.status {
                    diff_core::FileStatus::Added | diff_core::FileStatus::Untracked => {
                        theme.addition
                    }
                    diff_core::FileStatus::Deleted => theme.deletion,
                    diff_core::FileStatus::Modified
                    | diff_core::FileStatus::Renamed
                    | diff_core::FileStatus::Copied => theme.ui.accent,
                };
                Paragraph::new(Line::from(vec![
                    Span::raw("  ".repeat(*depth)),
                    Span::styled(
                        file.status.code().to_string(),
                        Style::new().fg(status_color),
                    ),
                    Span::raw(format!(" {name}")),
                    Span::styled(
                        format!(" +{} -{}", file.additions(), file.deletions()),
                        Style::new().fg(theme.ui.text_muted),
                    ),
                ]))
                .render(content, buffer);
                file.staged
            }
        };
        Paragraph::new(stage_marker(entry_stage)).render(checkbox, buffer);

        if state.drawer_selected == index {
            buffer.set_style(
                row,
                Style::new()
                    .fg(theme.ui.canvas)
                    .bg(theme.ui.accent)
                    .add_modifier(if state.focus == FocusPane::Files {
                        Modifier::BOLD
                    } else {
                        Modifier::default()
                    }),
            );
        }
    }

    render_vertical_scrollbar(
        track,
        buffer,
        entry_count,
        usize::from(rows.height),
        state.drawer_scroll,
    );
    rows
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
    track: Rect,
    buffer: &mut Buffer,
    state: &mut DiffReviewState,
    theme: &RatatuiTheme,
) {
    if area.is_empty() {
        return;
    }
    state.last_height = usize::from(area.height).max(1);
    state.highlighter.prepare_viewport(state.last_height);
    if state.take_follow_request() {
        state.follow_selection();
    }
    let Some(visual_layout) = state.patch_visual_layout() else {
        return;
    };
    let last_scroll = visual_layout.len().saturating_sub(state.last_height);
    state.scroll = state.scroll.min(last_scroll);

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
    let layout = session.layout();

    for (drawn, visual_index) in (*scroll..visual_layout.len()).enumerate() {
        let y = area
            .y
            .saturating_add(u16::try_from(drawn).unwrap_or(u16::MAX));
        if y >= area.bottom() {
            break;
        }
        let row_area = Rect::new(area.x, y, area.width, 1);
        match visual_layout.row(visual_index) {
            Some(PatchVisualRow::Source(index)) => {
                let Some(row) = presentation.row(index) else {
                    continue;
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
                    row_area,
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
            }
            Some(PatchVisualRow::Annotation {
                source,
                annotation,
                line,
            }) => {
                render_annotation_line(row_area, buffer, theme, annotation, line);
                visible_rows.push((y, source));
                if let Some(column) = annotation.cursor_column(line) {
                    *cursor_position = Some(Position::new(
                        area.x
                            .saturating_add(column)
                            .min(area.right().saturating_sub(1)),
                        y,
                    ));
                }
            }
            None => break,
        }
    }
    render_vertical_scrollbar(
        track,
        buffer,
        visual_layout.len(),
        usize::from(area.height),
        *scroll,
    );
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
                        .fg(context.theme.ui.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("+{additions} -{deletions}"),
                    Style::new().fg(context.theme.ui.text_muted),
                ),
            ]))
            .render(area, buffer);
        }
        RowKind::HunkHeader | RowKind::Meta => {
            Paragraph::new(row.primary_cell().map_or("", |cell| cell.text.as_ref()))
                .style(
                    Style::new()
                        .fg(context.theme.ui.text_muted)
                        .bg(context.theme.ui.canvas),
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
        context.theme.ui.surface_selected
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
    let indicator = match tone {
        DiffTone::Added | DiffTone::Removed => '▌',
        DiffTone::Context | DiffTone::Meta => ' ',
    };
    let gutter = format!(
        "{indicator}{number:>width$} ",
        width = usize::from(GUTTER_WIDTH) - 2,
    );
    let highlights = crate::diff_preview::cell_highlights(
        context.highlighter,
        context.diff_theme,
        context.presentation,
        context.row,
        cell,
    );
    let gutter_foreground = match tone {
        DiffTone::Added => context.theme.addition,
        DiffTone::Removed => context.theme.deletion,
        DiffTone::Context | DiffTone::Meta => context.theme.gutter,
    };
    let mut spans = vec![Span::styled(
        gutter,
        Style::new().fg(gutter_foreground).bg(background),
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

fn render_footer(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut DiffReviewState,
    theme: &RatatuiTheme,
) {
    if area.is_empty() {
        return;
    }
    if let Some(prompt) = &state.repository_prompt {
        match prompt {
            RepositoryPrompt::Commit { message } => {
                let prefix = "commit › ";
                Paragraph::new(format!("{prefix}{message}"))
                    .style(Style::new().fg(theme.ui.text))
                    .render(area, buffer);
                let x = area
                    .x
                    .saturating_add(u16::try_from(prefix.len() + message.len()).unwrap_or(u16::MAX))
                    .min(area.right().saturating_sub(1));
                state.cursor_position = Some(Position::new(x, area.y));
            }
            RepositoryPrompt::Discard { path, status } => {
                Paragraph::new(format!(
                    "Discard all staged and unstaged changes to {path} ({status:?})? [y/N]"
                ))
                .style(Style::new().fg(theme.deletion))
                .render(area, buffer);
            }
        }
        return;
    }
    if let RepositoryOperationStatus::Error(message) = &state.repository_status {
        EmptyState::new(message, NoticeTone::Error, theme).render(area, buffer);
        return;
    }
    let hint = if matches!(state.repository_status, RepositoryOperationStatus::Pending) {
        "Git operation in progress…"
    } else if state.session.draft().is_some() {
        "[Enter] save  [Shift-Enter] newline  [Esc] cancel"
    } else if state.focus == FocusPane::Files {
        "[j/k] entry  [h/l] fold/open  [t] theme  [?] help"
    } else if state.layout().is_split() {
        "[j/k] line  [←/→] side  [c] comment  [s] submit  [t] theme  [?] help"
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
    ActionBar::new(
        Line::from(vec![
            Span::styled(hint, Style::new().fg(theme.ui.text_muted)),
            Span::styled(status, Style::new().fg(theme.ui.accent)),
        ]),
        theme,
    )
    .render(area, buffer);
}

fn render_help(area: Rect, buffer: &mut Buffer, theme: &RatatuiTheme) {
    let content = Modal::new("Review shortcuts", theme)
        .hint("? / Esc to close")
        .render(area, buffer);
    render_modal_text(
        content,
        buffer,
        "Navigation\n  j/k or arrows   move selection\n  h/l             pane or fold/open\n  Tab             change pane\n  ←/→ in split    change column\n  PgUp/PgDn       move a page\n\nGit\n  Space            stage/unstage file or directory\n  a/A              stage/unstage all\n  C/d              commit/discard file\n  r                refresh\n\nReview\n  c/e/x            add/edit/delete comment\n  s/y              submit/copy review\n  t                select theme\n  Esc              cancel or close",
        theme,
    );
}
