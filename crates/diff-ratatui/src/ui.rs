//! Renderer-native components backed by shared semantic design roles.

use crate::{RatatuiTheme, RatatuiUiTheme};
use diff_theme::ControlState;
pub use diff_theme::{ButtonVariant, ModalSize, NoticeTone, SelectionState};
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
    theme: &'a RatatuiTheme,
}
impl<'a> AppFrame<'a> {
    pub(crate) const fn new(title: &'a str, borders: bool, theme: &'a RatatuiTheme) -> Self {
        Self {
            title,
            borders,
            theme,
        }
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
            Constraint::Length(ACTION_BAR_HEIGHT.min(inner.height)),
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
        Paragraph::new(self.line)
            .style(
                Style::new()
                    .fg(self.theme.ui.text_muted)
                    .bg(self.theme.ui.surface),
            )
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

pub(crate) struct SelectableRow<'a> {
    line: Line<'a>,
    state: SelectionState,
    theme: &'a RatatuiTheme,
}
impl<'a> SelectableRow<'a> {
    pub(crate) const fn new(
        line: Line<'a>,
        state: SelectionState,
        theme: &'a RatatuiTheme,
    ) -> Self {
        Self { line, state, theme }
    }
}
impl Widget for SelectableRow<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        Paragraph::new(self.line)
            .style(self.theme.ui.selection_style(self.state))
            .render(area, buffer);
    }
}

pub(crate) struct Modal<'a> {
    title: &'a str,
    hint: Option<&'a str>,
    size: ModalSize,
    theme: &'a RatatuiTheme,
}
impl<'a> Modal<'a> {
    pub(crate) const fn new(title: &'a str, theme: &'a RatatuiTheme) -> Self {
        Self {
            title,
            hint: None,
            size: ModalSize::Medium,
            theme,
        }
    }
    pub(crate) const fn hint(mut self, hint: &'a str) -> Self {
        self.hint = Some(hint);
        self
    }
    pub(crate) const fn size(mut self, size: ModalSize) -> Self {
        self.size = size;
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
            .style(
                Style::new()
                    .fg(self.theme.ui.text)
                    .bg(self.theme.ui.surface),
            )
            .border_style(Style::new().fg(self.theme.ui.border));
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
        .style(Style::new().fg(theme.ui.text).bg(theme.ui.surface))
        .render(area, buffer);
}

#[cfg(test)]
mod tests {
    use super::*;
    use diff_theme::DiffTheme;

    #[test]
    fn component_gallery_renders_shared_states() {
        let theme = RatatuiTheme::from(&DiffTheme::default());
        let area = Rect::new(0, 0, 48, 16);
        let mut buffer = Buffer::empty(area);
        let regions = AppFrame::new("Components", true, &theme).render(area, &mut buffer);
        SelectableRow::new(Line::from("Selected row"), SelectionState::Focused, &theme).render(
            Rect::new(regions.body.x, regions.body.y, regions.body.width, 1),
            &mut buffer,
        );
        ActionLabel::new("a", "Approve", &theme)
            .variant(ButtonVariant::Primary)
            .render(regions.footer, &mut buffer);
        let modal = Modal::new("Dialog", &theme)
            .hint("Esc close")
            .render(area, &mut buffer);
        EmptyState::new("Problem", NoticeTone::Error, &theme).render(modal, &mut buffer);
        assert!(buffer.content().iter().any(|cell| cell.symbol() == "A"));
        assert!(buffer.content().iter().any(|cell| cell.symbol() == "P"));
    }
}
