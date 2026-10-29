//! Visible-range Ratatui rendering.

use crate::{
    DiffReviewState, DiffReviewStatus, FocusPane, RatatuiTheme,
    annotation::render_annotation_line,
    color::{native_color, page_color},
    commit_picker::render_commit_picker,
    definition_card::NameOverlays,
    palette::render_palette,
    patch_layout::{PatchContentLayout, PatchRowGeometry, PatchVisualRow},
    state::{CodeCell, HitLayout, Overlay, RepositoryPrompt},
    style::{diff_indicator, file_status_color, stat_spans},
    syntax::styled_spans,
    text::{FitOptions, fit_spans_from},
    theme_picker::render_theme_picker,
    toolbar::{render_review_bar, render_toolbar},
    ui::{AppFrame, EmptyState, Modal, ModalSize, NoticeTone, Notification, SelectionState},
    widgets::{render_vertical_scrollbar, rows_and_track},
};
use clankerdiff_core::{DiffSide, DiffTone, FileDiff, FileTree, GapInfo, PresentedCell, PresentedRow, RowKind};
use clankerdiff_syntax::SyntaxHighlighter;
use clankerdiff_ui_core::ReviewTheme;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, StatefulWidget, Widget},
};

const DRAWER_BREAKPOINT: u16 = 72;
pub(crate) const GAP_EXPAND_WIDTH: u16 = 3;
pub(crate) const FILE_CHEVRON_WIDTH: u16 = 2;
const DRAWER_MIN_WIDTH: u16 = 20;
const DRAWER_MAX_WIDTH: u16 = 36;
const TOOLBAR_HEIGHT: u16 = 1;

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
        Self { title: "Diff Review".to_owned(), borders: true }
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
        state.patch_hits.clear();
        state.hits.clear();
        let theme = RatatuiTheme::from(&state.theme);
        let regions = AppFrame::new(Line::from(self.title.as_str()), self.borders, &theme)
            .footer(state.options.footer)
            .render(area, buffer);
        let [toolbar, body] =
            Layout::vertical([Constraint::Length(TOOLBAR_HEIGHT.min(regions.body.height)), Constraint::Min(0)])
                .areas(regions.body);
        render_toolbar(toolbar, buffer, state, &theme);
        render_body(body, buffer, state, &theme);
        render_review_bar(regions.footer, buffer, state, &theme);
        state.render_definition_card(buffer, &theme);
        render_notification(body, buffer, state, &theme);
        let overlay = match &state.overlay {
            Some(Overlay::Repository(_)) => render_repository_prompt(area, buffer, state, &theme),
            Some(Overlay::Commands(_)) => render_palette(area, buffer, state, &theme),
            Some(Overlay::Commits(_)) => render_commit_picker(area, buffer, state, &theme),
            Some(Overlay::Themes(picker)) => Some(render_theme_picker(area, buffer, picker, &theme, state.pointer)),
            None => None,
        };
        state.hit_layout.overlay = overlay.map(outset);
        state.dirty = false;
    }
}

const fn outset(area: Rect) -> Rect {
    Rect::new(area.x.saturating_sub(1), area.y.saturating_sub(1), area.width + 2, area.height + 2)
}

fn render_body(area: Rect, buffer: &mut Buffer, state: &mut DiffReviewState, theme: &RatatuiTheme) {
    let notice = match &state.status {
        DiffReviewStatus::Loading => Some(("Loading diff…".to_owned(), NoticeTone::Info)),
        DiffReviewStatus::Error(message) => Some((format!("Diff unavailable: {message}"), NoticeTone::Error)),
        DiffReviewStatus::Ready if state.document().files.is_empty() => {
            Some((format!("No changes ({})", state.target()), NoticeTone::Neutral))
        }
        DiffReviewStatus::Ready => None,
    };
    match notice {
        Some((text, tone)) => EmptyState::new(&text, tone, theme).render(area, buffer),
        None => render_document(area, buffer, state, theme),
    }
}

fn render_document(area: Rect, buffer: &mut Buffer, state: &mut DiffReviewState, theme: &RatatuiTheme) {
    let drawer_width = if state.sidebar_hidden {
        0
    } else {
        state.options.navigation.width(
            area.width,
            DRAWER_BREAKPOINT,
            (area.width / 3).clamp(DRAWER_MIN_WIDTH, DRAWER_MAX_WIDTH),
        )
    };
    let (drawer, patch) = if drawer_width > 0 {
        let [drawer, separator, patch] =
            Layout::horizontal([Constraint::Length(drawer_width), Constraint::Length(1), Constraint::Min(1)])
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

fn render_drawer(area: Rect, buffer: &mut Buffer, state: &mut DiffReviewState, theme: &RatatuiTheme) -> Rect {
    let (rows, track) = rows_and_track(area, true);
    state.hit_layout.drawer_stage_column = rows.width.checked_sub(1).map(|offset| rows.x.saturating_add(offset));
    state.drawer_height = usize::from(rows.height).max(1);
    let entry_count = state.drawer.entries().len();
    state.drawer_scroll = state.drawer_scroll.min(entry_count.saturating_sub(state.drawer_height));
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
    for (offset, entry) in state.drawer.entries().iter().skip(state.drawer_scroll).take(state.drawer_height).enumerate()
    {
        let y = rows.y.saturating_add(u16::try_from(offset).unwrap_or(u16::MAX));
        let index = state.drawer_scroll.saturating_add(offset);
        let row = Rect::new(rows.x, y, rows.width, 1);
        let indent = "  ".repeat(entry.depth);
        let (left, right, entry_stage) = match entry.file {
            None => {
                let marker = if entry.expanded { "▾" } else { "▸" };
                (
                    vec![
                        Span::styled(format!("{indent}{marker} "), Style::new().fg(theme.ui.text_muted)),
                        Span::styled(entry.name.clone(), Style::new().fg(theme.ui.text)),
                    ],
                    Vec::new(),
                    FileTree::stage_state_for(state.document(), &entry.path),
                )
            }
            Some(file_index) => {
                let Some(file) = state.document().files.get(file_index) else {
                    continue;
                };
                let status = Style::new().fg(file_status_color(file.status, theme));
                (
                    vec![
                        Span::raw(format!("{indent}  ")),
                        Span::styled(entry.name.clone(), Style::new().fg(theme.ui.text)),
                    ],
                    [
                        stat_spans(file.additions(), file.deletions(), theme).to_vec(),
                        vec![Span::styled(format!(" {}", file.status.code()), status)],
                    ]
                    .concat(),
                    file.staged,
                )
            }
        };
        let mut right = right;
        right.push(Span::styled(format!(" {}", stage_marker(entry_stage)), Style::new().fg(theme.ui.text_muted)));
        let right = Line::from(right);
        let right_width = u16::try_from(right.width()).unwrap_or(u16::MAX).min(row.width);
        let [content, trailing] = Layout::horizontal([Constraint::Min(0), Constraint::Length(right_width)]).areas(row);
        Paragraph::new(Line::from(left)).render(content, buffer);
        Paragraph::new(right).render(trailing, buffer);

        if selected == Some(index) {
            buffer.set_style(
                row,
                Style::new()
                    .fg(theme.ui.accent_foreground)
                    .bg(theme.ui.accent)
                    .add_modifier(if state.focus == FocusPane::Files { Modifier::BOLD } else { Modifier::default() }),
            );
        } else if state.pointer.is_some_and(|pointer| row.contains(pointer)) {
            buffer.set_style(row, Style::new().bg(theme.ui.surface_hover));
        }
    }

    render_vertical_scrollbar(track, buffer, entry_count, usize::from(rows.height), state.drawer_scroll);
    rows
}

const fn stage_marker(state: clankerdiff_core::StageState) -> &'static str {
    match state {
        clankerdiff_core::StageState::Unstaged => "☐",
        clankerdiff_core::StageState::Staged => "☑",
        clankerdiff_core::StageState::PartiallyStaged => "◩",
    }
}

fn render_patch(area: Rect, track: Rect, buffer: &mut Buffer, state: &mut DiffReviewState, theme: &RatatuiTheme) {
    if area.is_empty() || render_reference_status(area, buffer, state, theme) {
        return;
    }
    state.last_height = usize::from(area.height).max(1);
    state.apply_pending_scroll();
    let Some(visual_layout) = state.patch_visual_layout() else {
        return;
    };
    state.scroll = state.scroll.min(visual_layout.len().saturating_sub(state.last_height));
    let active_file = state.active_file();
    let top = visual_layout.row(state.scroll).map(|row| row.source());
    let symbol_path = top.map(|top| state.symbol_path(top)).unwrap_or_default();
    let names = state.name_overlays();
    let pointer = state.pointer;

    let DiffReviewState {
        session,
        theme: diff_theme,
        highlighter,
        focus,
        scroll,
        patch_hits,
        cursor_position,
        options,
        ..
    } = state;
    patch_hits.clear();
    let presentation = session.presentation();
    let selected_row = session.selected_row();
    let selected_side = session.selected_side();
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
                height = u16::try_from((range.len() - segment).min(usize::from(area.bottom() - y))).unwrap_or(u16::MAX);
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
                    names: &names,
                    code_cells: &mut patch_hits.code_cells,
                    pointer,
                };
                let selected = selected_row == Some(index) && *focus == FocusPane::Diff;
                render_row(
                    Rect::new(area.x, y, area.width, height),
                    buffer,
                    &mut context,
                    &RowStyle { selected, selected_side, source_label },
                );
                patch_hits.rows.extend((y..y + height).map(|screen_y| (screen_y, index)));
            }
            Some(PatchVisualRow::Annotation { source, annotation, line }) => {
                render_annotation_line(row_area, buffer, theme, annotation, line);
                patch_hits.rows.push((y, source));
                if let Some(column) = annotation.cursor_column(line) {
                    *cursor_position =
                        Some(Position::new(area.x.saturating_add(column).min(area.right().saturating_sub(1)), y));
                }
            }
            None => break,
        }
        visual_index += usize::from(height);
        y += height;
    }
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
            FileDividerState {
                collapsed: presentation.is_collapsed(file),
                selected: selected_row == Some(header) && *focus == FocusPane::Diff,
                symbol_path: &symbol_path,
                pointer,
            },
        );
        patch_hits.replace_row(area.y, header);
    }
    render_vertical_scrollbar(track, buffer, visual_layout.len(), usize::from(area.height), *scroll);
}

fn render_reference_status(area: Rect, buffer: &mut Buffer, state: &mut DiffReviewState, theme: &RatatuiTheme) -> bool {
    let Some(status) = state.reference_status() else {
        return false;
    };
    state.patch_hits.clear();
    Paragraph::new(status)
        .style(Style::new().fg(theme.ui.text_muted).bg(theme.ui.canvas))
        .alignment(Alignment::Center)
        .render(area, buffer);
    true
}

fn render_gap_row(area: Rect, buffer: &mut Buffer, context: &CellContext<'_>, selected: bool) {
    let theme = context.theme;
    let selected = selected || context.pointer.is_some_and(|pointer| area.contains(pointer));
    let info = context.presentation.gap_info(context.source);
    let rule = Style::new().fg(if selected { theme.ui.accent } else { theme.ui.border });
    let mut spans = vec![Span::styled(
        if info.is_some_and(GapInfo::is_expandable) { " ↕ " } else { "   " },
        Style::new().fg(theme.ui.accent),
    )];
    if selected && let Some(info) = info {
        spans.push(Span::styled("── ", rule));
        spans.push(Span::styled(info.hint(), Style::new().fg(theme.ui.text_muted)));
        spans.push(Span::raw(" "));
    }
    let used = spans.iter().map(Span::width).sum::<usize>();
    spans.push(Span::styled("─".repeat(usize::from(area.width).saturating_sub(used)), rule));
    Paragraph::new(Line::from(spans))
        .style(Style::new().bg(if selected { theme.ui.surface_selected } else { theme.ui.canvas }))
        .render(area, buffer);
}

#[derive(Clone, Copy)]
struct FileDividerState<'a> {
    collapsed: bool,
    selected: bool,
    symbol_path: &'a [String],
    pointer: Option<Position>,
}

fn render_file_divider(
    area: Rect,
    buffer: &mut Buffer,
    theme: &RatatuiTheme,
    file: &FileDiff,
    source_label: Option<&str>,
    state: FileDividerState<'_>,
) {
    Clear.render(area, buffer);
    buffer.set_style(area, Style::new().bg(if state.selected { theme.ui.surface_selected } else { theme.ui.surface }));
    let chevron = match (source_label, state.collapsed) {
        (Some(_), _) => "",
        (None, true) => " ▸",
        (None, false) => " ▾",
    };
    let chevron_area = Rect::new(area.x, area.y, FILE_CHEVRON_WIDTH.min(area.width), 1);
    let chevron_hovered = state.pointer.is_some_and(|pointer| chevron_area.contains(pointer));
    let mut left = vec![
        Span::styled(chevron, Style::new().fg(if chevron_hovered { theme.ui.accent } else { theme.ui.text_muted })),
        Span::styled(format!(" {} ", file.status.code()), Style::new().fg(file_status_color(file.status, theme))),
        Span::styled(file.path.file_name().to_owned(), Style::new().fg(theme.ui.text).add_modifier(Modifier::BOLD)),
    ];
    if let Some(directory) = file.path.directory() {
        left.push(Span::styled(format!(" {directory}/"), Style::new().fg(theme.ui.text_muted)));
    }
    if let Some(old) = file.renamed_from() {
        left.push(Span::styled(format!(" ← {old}"), Style::new().fg(theme.ui.text_muted)));
    }
    for name in state.symbol_path {
        left.push(Span::styled(" › ", Style::new().fg(theme.ui.text_muted)));
        left.push(Span::styled(name.clone(), Style::new().fg(theme.ui.text_secondary)));
    }
    let right = source_label.map_or_else(
        || Line::from([stat_spans(file.additions(), file.deletions(), theme).to_vec(), vec![Span::raw(" ")]].concat()),
        |label| Line::styled(format!("{label} "), Style::new().fg(theme.ui.text_muted)),
    );
    let right_width = u16::try_from(right.width()).unwrap_or(u16::MAX).min(area.width);
    let [left_area, right_area] = Layout::horizontal([Constraint::Min(0), Constraint::Length(right_width)]).areas(area);
    Paragraph::new(Line::from(left)).render(left_area, buffer);
    Paragraph::new(right).render(right_area, buffer);
}

struct RowStyle {
    selected: bool,
    selected_side: DiffSide,
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
    names: &'a NameOverlays,
    code_cells: &'a mut Vec<CodeCell>,
    pointer: Option<Position>,
}

impl CellContext<'_> {
    fn geometry(&self) -> PatchRowGeometry {
        self.patch_layout.geometry(self.presentation.file_layout(self.row.file_index))
    }
}

fn render_row(area: Rect, buffer: &mut Buffer, context: &mut CellContext<'_>, style: &RowStyle) {
    let row = context.row;
    let geometry = context.geometry();
    match row.kind {
        RowKind::FileHeader => render_file_divider(
            area,
            buffer,
            context.theme,
            &context.presentation.document().files[row.file_index],
            style.source_label,
            FileDividerState {
                collapsed: context.presentation.is_collapsed(row.file_index),
                selected: style.selected,
                symbol_path: &[],
                pointer: context.pointer,
            },
        ),
        RowKind::Meta => {
            Paragraph::new(row.primary_cell().map_or("", |cell| cell.text.as_ref()))
                .style(Style::new().fg(context.theme.ui.text_muted).bg(context.theme.ui.canvas))
                .render(area, buffer);
        }
        RowKind::ExpandGap => render_gap_row(area, buffer, context, style.selected),
        RowKind::Code | RowKind::ExpandedContext if geometry.split => {
            let separator_width = u16::from(area.width > 0);
            let [left, separator, right] = Layout::horizontal([
                Constraint::Length(geometry.left.width),
                Constraint::Length(separator_width),
                Constraint::Length(geometry.right.width),
            ])
            .areas(area);
            let focused = |side| style.selected && style.selected_side == side;
            render_cell(left, buffer, context, row.left.as_ref(), focused(DiffSide::Old), DiffSide::Old);
            render_separator(separator, buffer, context.theme);
            render_cell(right, buffer, context, row.right.as_ref(), focused(DiffSide::New), DiffSide::New);
        }
        RowKind::Code | RowKind::ExpandedContext => {
            render_cell(area, buffer, context, row.primary_cell(), style.selected, DiffSide::New);
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
    let commentable = cell.is_some_and(|cell| cell.patch_source.is_some());
    let hovered = commentable && context.pointer.is_some_and(|pointer| area.contains(pointer));
    let background = if selected || hovered {
        page_color(context.diff_theme, context.diff_theme.diff.selection)
    } else {
        tone_background
    };
    buffer.set_style(area, Style::new().fg(foreground).bg(background));
    let Some(cell) = cell else {
        return;
    };
    for y in area.top()..area.bottom() {
        Paragraph::new(Line::from(diff_indicator(tone, context.diff_theme, Style::new().bg(background))))
            .render(Rect::new(area.x, y, area.width.min(1), 1), buffer);
    }
    let Some(checkpoint) = context.patch_layout.checkpoint(context.source, side, context.first_segment) else {
        return;
    };
    let geometry = context.geometry().pane(side);
    let highlights = crate::diff_preview::cell_highlights(
        context.highlighter,
        &context.diff_theme.syntax,
        context.presentation,
        context.row,
        cell,
    );
    let card = Style::new().bg(native_color(context.diff_theme.carded_name(), background));
    let location = context.presentation.source_location(context.row, cell);
    let overlays = location.as_ref().map_or_else(Vec::new, |location| context.names.styles(location, card));
    let source_spans = styled_spans(&cell.text, &highlights, &overlays, Style::new().bg(background));
    let fit = FitOptions {
        width: geometry.content_width,
        wrap: true,
        tab_width: usize::from(context.tab_width),
        continuation: "",
    };
    let lines = fit_spans_from(source_spans, fit, checkpoint);
    let mut from = checkpoint;
    for (local_row, (line, end)) in lines.take(usize::from(area.height)).enumerate() {
        let segment = context.first_segment + local_row;
        let y = area.y + u16::try_from(local_row).unwrap_or(u16::MAX);
        let add_comment = hovered && segment == 0;
        let number = if add_comment {
            "+".to_owned()
        } else if segment == 0 {
            cell.line_number().map_or_else(String::new, |number| number.to_string())
        } else {
            "↪".to_owned()
        };
        let gutter = format!("{number:>width$} ", width = usize::from(geometry.gutter_width).saturating_sub(2));
        let gutter_foreground = match tone {
            DiffTone::Added => context.theme.addition,
            DiffTone::Removed => context.theme.deletion,
            DiffTone::Context | DiffTone::Meta => context.theme.gutter,
        };
        let gutter_foreground = if add_comment {
            context.theme.ui.accent
        } else if segment == 0 {
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
        let source_area =
            Rect::new(area.x.saturating_add(gutter_area.width), y, area.width.saturating_sub(gutter_area.width), 1);
        Paragraph::new(line).render(source_area, buffer);
        if let Some(location) = &location {
            context.code_cells.push(CodeCell {
                area: source_area,
                gutter: gutter_area,
                row: context.source,
                side,
                commentable,
                location: location.clone(),
                text: cell.text.clone(),
                fit,
                from,
            });
        }
        from = end;
    }
}

fn render_notification(body: Rect, buffer: &mut Buffer, state: &DiffReviewState, theme: &RatatuiTheme) {
    if matches!(state.overlay, Some(Overlay::Repository(_))) {
        return;
    }
    let Some(message) = state.repository_error() else {
        return;
    };
    let width = body.width.saturating_sub(4);
    let notification = Notification::error(message, theme);
    let height = notification.height(width).min(body.height);
    if width == 0 || height == 0 {
        return;
    }
    let area = Rect::new(body.x + 2, body.bottom().saturating_sub(height + 1).max(body.y), width, height);
    notification.render(area, buffer);
}

fn render_repository_prompt(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut DiffReviewState,
    theme: &RatatuiTheme,
) -> Option<Rect> {
    let Some(Overlay::Repository(prompt)) = &state.overlay else {
        return None;
    };
    let base = theme.ui.selection_style(SelectionState::None);
    let muted = base.fg(theme.ui.text_muted);
    let (title, lines) = match prompt {
        RepositoryPrompt::Commit { message } => (
            "Commit staged changes".to_owned(),
            vec![
                Line::from(vec![Span::styled("› ", muted), Span::styled(message.clone(), base)]),
                Line::default(),
                Line::styled("Enter to commit · Esc to cancel", muted),
            ],
        ),
        RepositoryPrompt::Discard { path, .. } => (
            format!("Discard changes to {path}?"),
            vec![
                Line::styled("This removes both staged and unstaged changes.", base.fg(theme.deletion)),
                Line::default(),
                Line::styled("y to confirm · n or Esc to cancel", muted),
            ],
        ),
    };
    let content = Modal::new(&title, ModalSize::Compact, theme)
        .height(u16::try_from(lines.len()).unwrap_or(u16::MAX))
        .width(area.width.saturating_sub(4).min(64))
        .render(area, buffer);
    if let RepositoryPrompt::Commit { message } = prompt {
        let column = u16::try_from(2 + unicode_width::UnicodeWidthStr::width(message.as_str())).unwrap_or(u16::MAX);
        state.cursor_position =
            Some(Position::new(content.x.saturating_add(column).min(content.right().saturating_sub(1)), content.y));
    }
    Paragraph::new(lines).style(base).render(content, buffer);
    Some(content)
}
