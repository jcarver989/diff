//! Renderer-native components backed by shared semantic design roles.

use crate::{RatatuiTheme, RatatuiUiTheme};
use clankerdiff_theme::ControlState;
pub use clankerdiff_theme::{ButtonVariant, ModalSize, NoticeTone, SelectionState};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

pub(crate) const ACTION_BAR_HEIGHT: u16 = 1;

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameRegions {
    pub body: Rect,
    pub footer: Rect,
}

pub(crate) struct AppFrame<'a> {
    title: &'a str,
    borders: bool,
    footer: bool,
    theme: &'a RatatuiTheme,
}
impl<'a> AppFrame<'a> {
    pub(crate) const fn new(title: &'a str, borders: bool, theme: &'a RatatuiTheme) -> Self {
        Self {
            title,
            borders,
            footer: true,
            theme,
        }
    }
    pub(crate) const fn footer(mut self, footer: bool) -> Self {
        self.footer = footer;
        self
    }

    pub(crate) fn render(self, area: Rect, buffer: &mut Buffer) -> FrameRegions {
        buffer.set_style(
            area,
            Style::new().fg(self.theme.ui.text).bg(self.theme.ui.canvas),
        );
        let inner = if self.borders {
            let block = Block::new()
                .borders(Borders::ALL)
                .title(format!(" {} ", self.title))
                .border_style(Style::new().fg(self.theme.ui.accent));
            let inner = block.inner(area);
            block.render(area, buffer);
            inner
        } else {
            area
        };
        let [body, footer] = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(if self.footer {
                ACTION_BAR_HEIGHT.min(inner.height)
            } else {
                0
            }),
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
        Paragraph::new(self.line)
            .style(Style::new().fg(ui.text_muted).bg(ui.canvas))
            .render(area, buffer);
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
        Self {
            key,
            label,
            variant: ButtonVariant::Ghost,
            theme,
        }
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
            self.theme
                .ui
                .control_style(self.variant, ControlState::default()),
        )
    }
}
impl Widget for ActionLabel<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        Paragraph::new(self.into_span()).render(area, buffer);
    }
}

pub(crate) struct Modal<'a> {
    title: &'a str,
    hint: Option<&'a str>,
    size: ModalSize,
    theme: &'a RatatuiTheme,
}
impl<'a> Modal<'a> {
    pub(crate) const fn new(title: &'a str, size: ModalSize, theme: &'a RatatuiTheme) -> Self {
        Self {
            title,
            hint: None,
            size,
            theme,
        }
    }
    pub(crate) const fn hint(mut self, hint: &'a str) -> Self {
        self.hint = Some(hint);
        self
    }
    pub(crate) fn render(self, area: Rect, buffer: &mut Buffer) -> Rect {
        let (max_width, max_height) = RatatuiUiTheme::modal_size(self.size);
        let width = area.width.saturating_sub(4).min(max_width);
        let height = area.height.saturating_sub(4).min(max_height);
        let popup = Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        );
        Clear.render(popup, buffer);
        let block = Block::bordered()
            .title(format!(" {} ", self.title))
            .title_bottom(
                self.hint
                    .map_or_else(Line::default, |hint| Line::from(format!(" {hint} "))),
            )
            .style(self.theme.ui.selection_style(SelectionState::None))
            .border_style(
                Style::new().fg(self.theme.ui.on_background(self.theme.ui.surface).border),
            );
        let inner = block.inner(popup);
        block.render(popup, buffer);
        inner
    }
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
            .style(
                self.theme
                    .ui
                    .notice_style(self.tone)
                    .bg(self.theme.ui.canvas),
            )
            .render(area, buffer);
    }
}

pub(crate) fn render_modal_text(
    area: Rect,
    buffer: &mut Buffer,
    text: impl Into<Text<'static>>,
    theme: &RatatuiTheme,
) {
    Paragraph::new(text)
        .style(theme.ui.selection_style(SelectionState::None))
        .render(area, buffer);
}
