//! Renderer-native components backed by shared semantic design roles.

use crate::{RatatuiTheme, RatatuiUiTheme};
pub use clankerdiff_ui_core::{ButtonVariant, ModalSize, NoticeTone, SelectionState};
use clankerdiff_ui_core::{ControlState, InteractionState, PickerRow, PickerState};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Position, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};
use unicode_width::UnicodeWidthStr;

pub(crate) const ACTION_BAR_HEIGHT: u16 = 1;

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameRegions {
    pub body: Rect,
    pub footer: Rect,
}

pub(crate) struct AppFrame<'a> {
    title: Line<'a>,
    borders: bool,
    footer: bool,
    theme: &'a RatatuiTheme,
}
impl<'a> AppFrame<'a> {
    pub(crate) fn new(title: impl Into<Line<'a>>, borders: bool, theme: &'a RatatuiTheme) -> Self {
        Self { title: title.into(), borders, footer: true, theme }
    }
    pub(crate) const fn footer(mut self, footer: bool) -> Self {
        self.footer = footer;
        self
    }

    pub(crate) fn render(self, area: Rect, buffer: &mut Buffer) -> FrameRegions {
        buffer.set_style(area, Style::new().fg(self.theme.ui.text).bg(self.theme.ui.canvas));
        let inner = if self.borders {
            let padded = |line: Line<'a>| Line::from([vec![Span::raw(" ")], line.spans, vec![Span::raw(" ")]].concat());
            let block = Block::new()
                .borders(Borders::ALL)
                .title(padded(self.title))
                .border_style(Style::new().fg(self.theme.ui.accent));
            let inner = block.inner(area);
            block.render(area, buffer);
            inner
        } else {
            area
        };
        let [body, footer] = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(if self.footer { ACTION_BAR_HEIGHT.min(inner.height) } else { 0 }),
        ])
        .areas(inner);
        FrameRegions { body, footer }
    }
}

pub(crate) struct ActionBar<'a> {
    line: Line<'a>,
    theme: &'a RatatuiTheme,
}
impl<'a> ActionBar<'a> {
    pub(crate) const fn new(line: Line<'a>, theme: &'a RatatuiTheme) -> Self {
        Self { line, theme }
    }
}
impl Widget for ActionBar<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        let ui = self.theme.ui.on_background(self.theme.ui.surface);
        Paragraph::new(self.line).style(Style::new().fg(ui.text_muted).bg(ui.canvas)).render(area, buffer);
    }
}

/// Terminal representation of the same semantic action used by graphical buttons.
pub struct ActionLabel<'a> {
    key: &'a str,
    label: &'a str,
    variant: ButtonVariant,
    theme: &'a RatatuiTheme,
}
impl<'a> ActionLabel<'a> {
    #[must_use]
    pub const fn new(key: &'a str, label: &'a str, theme: &'a RatatuiTheme) -> Self {
        Self { key, label, variant: ButtonVariant::Ghost, theme }
    }
    #[must_use]
    pub const fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    #[must_use]
    pub fn into_span(self) -> Span<'static> {
        Span::styled(
            format!("[{}] {}", self.key, self.label),
            self.theme.ui.control_style(self.variant, ControlState::default()),
        )
    }
}
impl Widget for ActionLabel<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        Paragraph::new(self.into_span()).render(area, buffer);
    }
}

pub(crate) struct Button<'a> {
    label: &'a str,
    variant: ButtonVariant,
    state: ControlState,
    theme: &'a RatatuiTheme,
}
impl<'a> Button<'a> {
    pub(crate) const fn new(label: &'a str, theme: &'a RatatuiTheme) -> Self {
        Self { label, variant: ButtonVariant::Ghost, state: ControlState::new(InteractionState::Rest), theme }
    }
    pub(crate) const fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
    pub(crate) const fn selected(mut self, selected: bool) -> Self {
        self.state.selected = selected;
        self
    }
    pub(crate) const fn disabled(mut self, disabled: bool) -> Self {
        if disabled {
            self.state.interaction = InteractionState::Disabled;
        }
        self
    }
    pub(crate) fn hovered_at(mut self, area: Rect, pointer: Option<Position>) -> Self {
        if self.state.interaction == InteractionState::Rest && pointer.is_some_and(|pointer| area.contains(pointer)) {
            self.state.interaction = InteractionState::Hovered;
        }
        self
    }
    pub(crate) fn style(&self) -> Style {
        let style = self.theme.ui.control_style(self.variant, self.state);
        if self.variant == ButtonVariant::Ghost && self.state.interaction == InteractionState::Disabled {
            return Style::new().fg(self.theme.ui.border);
        }
        style
    }
}
impl Widget for Button<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        Paragraph::new(format!(" {} ", self.label)).style(self.style()).render(area, buffer);
    }
}

pub(crate) struct Notification<'a> {
    message: &'a str,
    tone: NoticeTone,
    theme: &'a RatatuiTheme,
}
impl<'a> Notification<'a> {
    pub(crate) const fn error(message: &'a str, theme: &'a RatatuiTheme) -> Self {
        Self { message, tone: NoticeTone::Error, theme }
    }
    pub(crate) fn height(&self, width: u16) -> u16 {
        let inner = usize::from(width.saturating_sub(4)).max(1);
        u16::try_from(self.message.width().div_ceil(inner).max(1) + 2).unwrap_or(u16::MAX)
    }
}
impl Widget for Notification<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        let tone = self.theme.ui.notice_style(self.tone);
        let block = Block::bordered()
            .padding(ratatui::widgets::Padding::horizontal(1))
            .style(self.theme.ui.selection_style(SelectionState::None))
            .border_style(Style::new().fg(tone.fg.unwrap_or(self.theme.ui.destructive)));
        Clear.render(area, buffer);
        let inner = block.inner(area);
        block.render(area, buffer);
        Paragraph::new(self.message)
            .style(tone.bg(self.theme.ui.surface))
            .wrap(Wrap { trim: true })
            .render(inner, buffer);
    }
}

pub(crate) struct Modal<'a> {
    title: &'a str,
    hint: Option<&'a str>,
    size: ModalSize,
    width: Option<u16>,
    height: Option<u16>,
    theme: &'a RatatuiTheme,
}
impl<'a> Modal<'a> {
    pub(crate) const fn new(title: &'a str, size: ModalSize, theme: &'a RatatuiTheme) -> Self {
        Self { title, hint: None, size, width: None, height: None, theme }
    }
    pub(crate) const fn hint(mut self, hint: &'a str) -> Self {
        self.hint = Some(hint);
        self
    }
    pub(crate) const fn width(mut self, width: u16) -> Self {
        self.width = Some(width);
        self
    }
    pub(crate) const fn height(mut self, inner_height: u16) -> Self {
        self.height = Some(inner_height.saturating_add(2));
        self
    }
    pub(crate) fn render(self, area: Rect, buffer: &mut Buffer) -> Rect {
        let (max_width, max_height) = RatatuiUiTheme::modal_size(self.size);
        let width = area.width.saturating_sub(4).min(self.width.unwrap_or(max_width));
        let height = area.height.saturating_sub(4).min(self.height.unwrap_or(max_height));
        let popup = Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        );
        Clear.render(popup, buffer);
        let block = Block::bordered()
            .title(format!(" {} ", self.title))
            .title_bottom(self.hint.map_or_else(Line::default, |hint| Line::from(format!(" {hint} "))))
            .style(self.theme.ui.selection_style(SelectionState::None))
            .border_style(Style::new().fg(self.theme.ui.on_background(self.theme.ui.surface).border));
        let inner = block.inner(popup);
        block.render(popup, buffer);
        inner
    }
}

pub(crate) struct PickerArea {
    pub cursor: Option<Position>,
    pub rows: Vec<(Rect, usize)>,
}

pub(crate) fn render_picker<'l>(
    area: Rect,
    buffer: &mut Buffer,
    theme: &RatatuiTheme,
    picker: &PickerState,
    query: Option<&str>,
    pointer: Option<Position>,
    lines: impl FnOnce(usize) -> Vec<Line<'l>>,
) -> PickerArea {
    const HIGHLIGHT: &str = "› ";
    let base = theme.ui.selection_style(SelectionState::None);
    let mut list = area;
    let cursor = query.filter(|_| !area.is_empty()).map(|query| {
        let prompt = Line::from(vec![Span::styled(HIGHLIGHT, base.fg(theme.ui.text_muted)), Span::raw(query)]);
        let column = u16::try_from(prompt.width()).unwrap_or(u16::MAX);
        Paragraph::new(prompt).style(base).render(Rect { height: 1, ..area }, buffer);
        list = Rect { y: area.y + 1, height: area.height - 1, ..area };
        Position::new(area.x.saturating_add(column).min(area.right().saturating_sub(1)), area.y)
    });
    if picker.matches().is_empty() {
        Paragraph::new(" No matches").style(base.fg(theme.ui.text_muted)).render(list, buffer);
    }
    let lines = lines(usize::from(list.width).saturating_sub(HIGHLIGHT.width()));
    let rows = picker.rows();
    let highlighted = picker.highlighted_position();
    let current =
        rows.iter().position(|row| matches!(row, PickerRow::Option { position, .. } if *position == highlighted));
    let offset = current.map_or(0, |row| (row + 1).saturating_sub(usize::from(list.height)));
    let mut hits = Vec::new();
    for (y, row) in (list.top()..list.bottom()).zip(rows.into_iter().skip(offset)) {
        let area = Rect { y, height: 1, ..list };
        let (index, style, marker) = match row {
            PickerRow::Group(title) => {
                Paragraph::new(format!(" {title}")).style(base.fg(theme.ui.accent)).render(area, buffer);
                continue;
            }
            PickerRow::Option { position, index } if position == highlighted => {
                (index, theme.ui.selection_style(SelectionState::Selected), HIGHLIGHT)
            }
            PickerRow::Option { index, .. } if pointer.is_some_and(|pointer| area.contains(pointer)) => {
                (index, base.bg(theme.ui.surface_hover), "  ")
            }
            PickerRow::Option { index, .. } => (index, base, "  "),
        };
        let line = lines.get(index).cloned().unwrap_or_default();
        Paragraph::new(Line::from([vec![Span::raw(marker)], line.spans].concat())).style(style).render(area, buffer);
        hits.push((area, index));
    }
    PickerArea { cursor, rows: hits }
}

pub(crate) struct EmptyState<'a> {
    text: &'a str,
    tone: NoticeTone,
    theme: &'a RatatuiTheme,
}
impl<'a> EmptyState<'a> {
    pub(crate) const fn new(text: &'a str, tone: NoticeTone, theme: &'a RatatuiTheme) -> Self {
        Self { text, tone, theme }
    }
}
impl Widget for EmptyState<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        Paragraph::new(self.text)
            .style(self.theme.ui.notice_style(self.tone).bg(self.theme.ui.canvas))
            .render(area, buffer);
    }
}

pub(crate) fn render_modal_text(area: Rect, buffer: &mut Buffer, text: impl Into<Text<'static>>, theme: &RatatuiTheme) {
    Paragraph::new(text).style(theme.ui.selection_style(SelectionState::None)).render(area, buffer);
}
