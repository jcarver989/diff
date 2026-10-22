//! Visible-range Ratatui rendering.

use crate::{
    DiffReviewState, DiffReviewStatus, FocusPane, RatatuiTheme, RepositoryOperationStatus,
    annotation::render_annotation_line,
    color::page_color,
    patch_layout::{PatchContentLayout, PatchVisualRow},
    state::{HitLayout, RepositoryPrompt},
    style::{diff_indicator, file_status_color, syntax_style},
    text::{FitOptions, fit_spans_from},
    theme_picker::render_theme_picker,
    ui::{ActionBar, AppFrame, EmptyState, Modal, ModalSize, NoticeTone, render_modal_text},
    widgets::{render_vertical_scrollbar, rows_and_track},
};
use clankerdiff_core::{
    DiffSide, DiffTone, FileDiff, FileTree, GapInfo, PresentedCell, PresentedRow, RowKind,
};
use clankerdiff_syntax::{HighlightSpan, SyntaxHighlighter};
use clankerdiff_theme::ReviewTheme;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, StatefulWidget, Widget},
};

const DRAWER_BREAKPOINT: u16 = 72;
pub(crate) const GAP_EXPAND_WIDTH: u16 = 3;
const DRAWER_MIN_WIDTH: u16 = 20;
const DRAWER_MAX_WIDTH: u16 = 36;

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
        state.hit_layout = HitLayout::default();
        state.visible_rows.clear();
        let theme = RatatuiTheme::from(&state.theme);
        let regions = AppFrame::new(&self.title, self.borders, &theme)
            .footer(state.options.footer || state.repository_prompt.is_some())
            .render(area, buffer);
        let body = regions.body;
        let footer = regions.footer;
        render_body(body, buffer, state, &theme);
        render_footer(footer, buffer, state, &theme);
        if state.help {
            render_help(area, buffer, state, &theme);
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
        DiffReviewStatus::Ready if state.document().files.is_empty() => Some((
            format!("No changes (scope: {})", state.scope()),
            NoticeTone::Neutral,
        )),
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
    let drawer_width = state.options.navigation.width(
        area.width,
        DRAWER_BREAKPOINT,
        (area.width / 3).clamp(DRAWER_MIN_WIDTH, DRAWER_MAX_WIDTH),
    );
    let (drawer, patch) = if drawer_width > 0 {
        let [drawer, separator, patch] = Layout::horizontal([
            Constraint::Length(drawer_width),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(area);
        render_separator(separator, buffer, theme);
        (drawer, patch)
    } else {
        (Rect::default(), area)
    };
    state.hit_layout.patch = patch;
    let (patch_rows, patch_track) = rows_and_track(patch, true);
    state.ensure_presentation(patch_rows.width);
    render_patch(patch_rows, patch_track, buffer, state, theme);
    state.follow_active_file();
    state.hit_layout.drawer = if drawer.is_empty() {
        state.hit_layout.drawer_stage_column = None;
        drawer
    } else {
        render_drawer(drawer, buffer, state, theme)
    };
}

fn render_separator(area: Rect, buffer: &mut Buffer, theme: &RatatuiTheme) {
    for y in area.top()..area.bottom() {
        Paragraph::new("│")
            .style(Style::new().fg(theme.ui.border).bg(theme.ui.canvas))
            .render(Rect::new(area.x, y, area.width, 1), buffer);
    }
}

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
    let selected = state.drawer.selected_position();
    if state.take_drawer_follow_request()
        && let Some(selected) = selected
    {
        if selected < state.drawer_scroll {
            state.drawer_scroll = selected;
        } else if selected >= state.drawer_scroll.saturating_add(state.drawer_height) {
            state.drawer_scroll = selected.saturating_sub(state.drawer_height.saturating_sub(1));
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
        let indent = "  ".repeat(entry.depth);
        let entry_stage = match entry.file {
            None => {
                let marker = if entry.expanded { "▾" } else { "▸" };
                Paragraph::new(Line::from(vec![
                    Span::raw(format!("{indent}{marker} ")),
                    Span::styled(format!("{}/", entry.name), Style::new().fg(theme.ui.accent)),
                ]))
                .render(content, buffer);
                FileTree::stage_state_for(state.document(), &entry.path)
            }
            Some(file_index) => {
                let Some(file) = state.document().files.get(file_index) else {
                    continue;
                };
                Paragraph::new(Line::from(vec![
                    Span::raw(indent),
                    Span::styled(
                        file.status.code().to_string(),
                        Style::new().fg(file_status_color(file.status, theme)),
                    ),
                    Span::raw(format!(" {}", entry.name)),
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

        if selected == Some(index) {
            buffer.set_style(
                row,
                Style::new()
                    .fg(theme.ui.accent_foreground)
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

const fn stage_marker(state: clankerdiff_core::StageState) -> &'static str {
    match state {
        clankerdiff_core::StageState::Unstaged => "☐",
        clankerdiff_core::StageState::Staged => "☑",
        clankerdiff_core::StageState::PartiallyStaged => "◩",
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "visible-row rendering is kept together"
)]
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
    state.apply_pending_scroll();
    let Some(visual_layout) = state.patch_visual_layout() else {
        return;
    };
    state.scroll = state
        .scroll
        .min(visual_layout.len().saturating_sub(state.last_height));
    let active_file = state.active_file();

    let DiffReviewState {
        session,
        theme: diff_theme,
        highlighter,
        focus,
        scroll,
        visible_rows,
        cursor_position,
        options,
        ..
    } = state;
    visible_rows.clear();
    let presentation = session.presentation();
    let selected_row = session.selected_row();
    let selected_side = session.selected_side();
    let layout = session.layout();
    let source_label = session.source_view_label();
    let mut visual_index = *scroll;
    let mut y = area.y;
    while y < area.bottom() {
        let mut height = 1;
        let row_area = Rect::new(area.x, y, area.width, 1);
        match visual_layout.row(visual_index) {
            Some(PatchVisualRow::Source { index, segment }) => {
                let Some(row) = presentation.row(index) else {
                    break;
                };
                let Some(range) = visual_layout.content().content_range_for_source(index) else {
                    break;
                };
                height = u16::try_from((range.len() - segment).min(usize::from(area.bottom() - y)))
                    .unwrap_or(u16::MAX);
                let mut context = CellContext {
                    theme,
                    diff_theme,
                    highlighter,
                    presentation,
                    row,
                    source: index,
                    first_segment: segment,
                    patch_layout: visual_layout.content(),
                    tab_width: options.tab_width,
                };
                let selected = selected_row == Some(index) && *focus == FocusPane::Diff;
                render_row(
                    Rect::new(area.x, y, area.width, height),
                    buffer,
                    &mut context,
                    &RowStyle {
                        selected,
                        selected_side,
                        layout,
                        source_label,
                    },
                );
                visible_rows.extend((y..y + height).map(|screen_y| (screen_y, index)));
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
        visual_index += usize::from(height);
        y += height;
    }
    let top = visual_layout.row(*scroll).map(|row| row.source());
    if let Some(file) = active_file
        && let Some(header) = presentation.file_range(file).map(|range| range.start)
        && top != Some(header)
    {
        render_file_divider(
            Rect::new(area.x, area.y, area.width, 1),
            buffer,
            theme,
            &presentation.document().files[file],
            source_label,
        );
        visible_rows.retain(|(screen_y, _)| *screen_y != area.y);
        visible_rows.push((area.y, header));
    }
    render_vertical_scrollbar(
        track,
        buffer,
        visual_layout.len(),
        usize::from(area.height),
        *scroll,
    );
}

fn render_gap_row(area: Rect, buffer: &mut Buffer, context: &CellContext<'_>, selected: bool) {
    let theme = context.theme;
    let info = context.presentation.gap_info(context.source);
    let rule = Style::new().fg(if selected {
        theme.ui.accent
    } else {
        theme.ui.border
    });
    let mut spans = vec![Span::styled(
        if info.is_some_and(GapInfo::is_expandable) {
            " ↕ "
        } else {
            "   "
        },
        Style::new().fg(theme.ui.accent),
    )];
    if selected && let Some(info) = info {
        spans.push(Span::styled("── ", rule));
        spans.push(Span::styled(
            info.hint(),
            Style::new().fg(theme.ui.text_muted),
        ));
        spans.push(Span::raw(" "));
    }
    let used = spans.iter().map(Span::width).sum::<usize>();
    spans.push(Span::styled(
        "─".repeat(usize::from(area.width).saturating_sub(used)),
        rule,
    ));
    Paragraph::new(Line::from(spans))
        .style(Style::new().bg(if selected {
            theme.ui.surface_selected
        } else {
            theme.ui.canvas
        }))
        .render(area, buffer);
}

fn render_file_divider(
    area: Rect,
    buffer: &mut Buffer,
    theme: &RatatuiTheme,
    file: &FileDiff,
    source_label: Option<&str>,
) {
    Clear.render(area, buffer);
    buffer.set_style(area, Style::new().bg(theme.ui.surface));
    let mut left = vec![
        Span::styled(
            format!(" {} ", file.status.code()),
            Style::new().fg(file_status_color(file.status, theme)),
        ),
        Span::styled(
            file.path.file_name().to_owned(),
            Style::new().fg(theme.ui.text).add_modifier(Modifier::BOLD),
        ),
    ];
    if let Some(directory) = file.path.directory() {
        left.push(Span::styled(
            format!(" {directory}/"),
            Style::new().fg(theme.ui.text_muted),
        ));
    }
    if let Some(old) = file.renamed_from() {
        left.push(Span::styled(
            format!(" ← {old}"),
            Style::new().fg(theme.ui.text_muted),
        ));
    }
    let right = source_label.map_or_else(
        || {
            Line::from(vec![
                Span::styled(
                    format!("+{}", file.additions()),
                    Style::new().fg(theme.addition),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("-{} ", file.deletions()),
                    Style::new().fg(theme.deletion),
                ),
            ])
        },
        |label| Line::styled(format!("{label} "), Style::new().fg(theme.ui.text_muted)),
    );
    let right_width = u16::try_from(right.width())
        .unwrap_or(u16::MAX)
        .min(area.width);
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(right_width)]).areas(area);
    Paragraph::new(Line::from(left)).render(left_area, buffer);
    Paragraph::new(right).render(right_area, buffer);
}

struct RowStyle {
    selected: bool,
    selected_side: DiffSide,
    layout: clankerdiff_core::Layout,
    source_label: Option<&'static str>,
}

struct CellContext<'a> {
    tab_width: u16,
    theme: &'a RatatuiTheme,
    diff_theme: &'a ReviewTheme,
    highlighter: &'a mut SyntaxHighlighter,
    presentation: &'a clankerdiff_core::DiffPresentation,
    row: &'a PresentedRow,
    source: usize,
    first_segment: usize,
    patch_layout: &'a PatchContentLayout,
}

fn render_row(area: Rect, buffer: &mut Buffer, context: &mut CellContext<'_>, style: &RowStyle) {
    let row = context.row;
    match row.kind {
        RowKind::FileHeader => render_file_divider(
            area,
            buffer,
            context.theme,
            &context.presentation.document().files[row.file_index],
            style.source_label,
        ),
        RowKind::Meta => {
            Paragraph::new(row.primary_cell().map_or("", |cell| cell.text.as_ref()))
                .style(
                    Style::new()
                        .fg(context.theme.ui.text_muted)
                        .bg(context.theme.ui.canvas),
                )
                .render(area, buffer);
        }
        RowKind::ExpandGap => render_gap_row(area, buffer, context, style.selected),
        RowKind::Code | RowKind::ExpandedContext if style.layout.is_split() => {
            let geometry = context.patch_layout.geometry();
            let separator_width = u16::from(area.width > 0);
            let [left, separator, right] = Layout::horizontal([
                Constraint::Length(geometry.left.width),
                Constraint::Length(separator_width),
                Constraint::Length(geometry.right.width),
            ])
            .areas(area);
            let focused = |side| style.selected && style.selected_side == side;
            render_cell(
                left,
                buffer,
                context,
                row.left.as_ref(),
                focused(DiffSide::Old),
                DiffSide::Old,
            );
            render_separator(separator, buffer, context.theme);
            render_cell(
                right,
                buffer,
                context,
                row.right.as_ref(),
                focused(DiffSide::New),
                DiffSide::New,
            );
        }
        RowKind::Code | RowKind::ExpandedContext => {
            render_cell(
                area,
                buffer,
                context,
                row.primary_cell(),
                style.selected,
                DiffSide::New,
            );
        }
    }
}

fn render_cell(
    area: Rect,
    buffer: &mut Buffer,
    context: &mut CellContext<'_>,
    cell: Option<&PresentedCell>,
    selected: bool,
    side: DiffSide,
) {
    let tone = cell.map_or(DiffTone::Context, |cell| cell.tone);
    let (foreground, tone_background) = context.theme.tone(tone);
    let background = if selected {
        page_color(context.diff_theme, context.diff_theme.diff.selection)
    } else {
        tone_background
    };
    buffer.set_style(area, Style::new().fg(foreground).bg(background));
    let Some(cell) = cell else {
        return;
    };
    for y in area.top()..area.bottom() {
        Paragraph::new(Line::from(diff_indicator(
            tone,
            context.diff_theme,
            Style::new().bg(background),
        )))
        .render(Rect::new(area.x, y, area.width.min(1), 1), buffer);
    }
    let Some(checkpoint) =
        context
            .patch_layout
            .checkpoint(context.source, side, context.first_segment)
    else {
        return;
    };
    let geometry = match side {
        DiffSide::Old => context.patch_layout.geometry().left,
        DiffSide::New => context.patch_layout.geometry().right,
    };
    let highlights = crate::diff_preview::cell_highlights(
        context.highlighter,
        &context.diff_theme.syntax,
        context.presentation,
        context.row,
        cell,
    );
    let source_spans = highlighted_spans(&cell.text, &highlights, background);
    let lines = fit_spans_from(
        source_spans,
        FitOptions {
            width: geometry.content_width,
            wrap: true,
            tab_width: usize::from(context.tab_width),
            continuation: "",
        },
        checkpoint,
    );
    for (local_row, (line, _)) in lines.take(usize::from(area.height)).enumerate() {
        let segment = context.first_segment + local_row;
        let y = area.y + u16::try_from(local_row).unwrap_or(u16::MAX);
        let number = if segment == 0 {
            cell.line_number()
                .map_or_else(String::new, |number| number.to_string())
        } else {
            "↪".to_owned()
        };
        let gutter = format!(
            "{number:>width$} ",
            width = usize::from(geometry.gutter_width).saturating_sub(2),
        );
        let gutter_foreground = match tone {
            DiffTone::Added => context.theme.addition,
            DiffTone::Removed => context.theme.deletion,
            DiffTone::Context | DiffTone::Meta => context.theme.gutter,
        };
        let gutter_foreground = if segment == 0 {
            gutter_foreground
        } else {
            context.theme.ui.text_muted
        };
        let gutter_area = Rect::new(area.x, y, geometry.gutter_width.min(area.width), 1);
        Paragraph::new(Line::from(vec![
            diff_indicator(tone, context.diff_theme, Style::new().bg(background)),
            Span::styled(gutter, Style::new().fg(gutter_foreground).bg(background)),
        ]))
        .render(gutter_area, buffer);
        let source_area = Rect::new(
            area.x.saturating_add(gutter_area.width),
            y,
            area.width.saturating_sub(gutter_area.width),
            1,
        );
        Paragraph::new(line).render(source_area, buffer);
    }
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
    if let Some(message) = state.repository_error() {
        EmptyState::new(message, NoticeTone::Error, theme).render(area, buffer);
        return;
    }
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
    let hint = if matches!(state.repository_status, RepositoryOperationStatus::Pending) {
        "Git operation in progress…".to_owned()
    } else if state.session.draft().is_some() {
        "[Enter] save  [Shift-Enter] newline  [Esc] cancel".to_owned()
    } else {
        state.footer_hint(usize::from(area.width).saturating_sub(status.len()))
    };
    ActionBar::new(
        Line::from(vec![
            Span::styled(hint, Style::new().fg(theme.ui.text_muted)),
            Span::styled(status, Style::new().fg(theme.ui.accent)),
        ]),
        theme,
    )
    .render(area, buffer);
}

fn render_help(area: Rect, buffer: &mut Buffer, state: &DiffReviewState, theme: &RatatuiTheme) {
    let content = Modal::new("Review shortcuts", ModalSize::Medium, theme)
        .hint("j/k scroll · ? / Esc close")
        .render(area, buffer);
    render_modal_text(
        content,
        buffer,
        state
            .help_bindings()
            .skip(state.help_scroll)
            .map(|binding| format!("{} ({:?})", binding.hint(), binding.scope))
            .collect::<Vec<_>>()
            .join("\n"),
        theme,
    );
}
