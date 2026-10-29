use crate::{
    DiffReviewCommand, DiffReviewEvent, DiffReviewState, FocusPane, InputOutcome, KeyModifiers, MarkdownLayoutOptions,
    MarkdownRenderer, RatatuiTheme,
    markdown::{MarkdownOptions, parse_markdown},
    syntax::highlighted_line,
    text::row_pieces,
    ui::SelectionState,
};
use clankerdiff_core::{LookupReply, RepoPath, SourceLocation};
use clankerdiff_symbols::{Activation, CardContent, DefinitionCard, Hold, Settled, SymbolIndex, SymbolReference};
use clankerdiff_syntax::LanguageHint;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph, Widget},
};
use std::{mem, ops::Range, sync::Arc};
use unicode_width::UnicodeWidthStr;

const CARD_MIN_WIDTH: u16 = 40;
const CARD_MAX_WIDTH: u16 = 88;
const CARD_CHROME: u16 = 4;
const DOC_LINES: usize = 12;

pub(crate) struct NameOverlays {
    card: Option<(SourceLocation, Range<usize>)>,
    links: Option<Arc<SymbolIndex>>,
}

impl NameOverlays {
    pub(crate) fn styles(&self, location: &SourceLocation, card: Style) -> Vec<(Range<usize>, Style)> {
        let underline = Style::new().add_modifier(Modifier::UNDERLINED);
        let marked =
            self.card.iter().filter(|(origin, _)| origin == location).map(|(_, columns)| (columns.clone(), card));
        let links = self.links.iter().flat_map(|index| index.references_at(location));
        marked.chain(links.map(|reference| (reference.span.columns.clone(), underline))).collect()
    }
}

struct Link {
    origin: SourceLocation,
    reference: Option<SymbolReference>,
}

pub(crate) fn links_held(modifiers: KeyModifiers) -> bool {
    modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) && !modifiers.contains(KeyModifiers::SHIFT)
}

impl DiffReviewState {
    pub(crate) fn set_links_visible(&mut self, visible: bool) -> bool {
        mem::replace(&mut self.links_visible, visible) != visible
    }

    pub(crate) fn show_definition_at_selection(&mut self) -> bool {
        let Some(origin) = self.session.selected_source_line() else {
            return false;
        };
        let index = self.symbols();
        self.names.open_at(origin, &self.session, &index, self.capabilities.lookups)
    }

    pub(crate) fn over_card(&self, position: Position) -> bool {
        self.hit_layout.card.is_some_and(|card| card.contains(position))
    }

    pub(crate) fn hover(&mut self, position: Position) -> bool {
        let opened = self.names.card().is_some();
        if self.over_card(position) {
            self.names.hover_card(true);
            return false;
        }
        self.names.hover_card(false);
        let link = self.link_at(position);
        let on_card_line = link.as_ref().zip(self.names.card()).is_some_and(|(link, card)| link.origin == card.origin);
        match &link {
            Some(Link { origin, reference: Some(reference) }) if self.card_shows(origin, reference) => {
                self.names.hover_token(true);
            }
            Some(Link { origin, reference: Some(reference) }) if self.links_visible => {
                self.show_definition(origin.clone(), reference, Hold::Hover);
                return true;
            }
            _ => self.names.hover_token(on_card_line),
        }
        if self.names.lingering() {
            return self.names.close();
        }
        opened != self.names.card().is_some()
    }

    pub(crate) fn follow_link(&mut self, position: Position) -> Option<InputOutcome<DiffReviewEvent>> {
        let Some(Link { origin, reference: Some(reference) }) = self.link_at(position) else {
            return None;
        };
        if self.card_shows(&origin, &reference) {
            self.names.pin();
        } else {
            self.show_definition(origin, &reference, Hold::Pinned);
        }
        self.focus = FocusPane::Diff;
        Some(self.go_to_definition().or_consumed())
    }

    pub(crate) fn go_to_definition(&mut self) -> InputOutcome<DiffReviewEvent> {
        let index = self.symbols();
        match self.names.activate(&index) {
            Activation::Wait => InputOutcome::Consumed,
            Activation::Jump(target) => {
                self.names.close();
                self.handle_command(DiffReviewCommand::JumpTo(target))
            }
            Activation::Idle => {
                self.names.close();
                InputOutcome::Consumed
            }
        }
    }

    pub(crate) fn next_definition(&mut self) -> bool {
        let index = self.symbols();
        self.names.cycle(&index)
    }

    fn show_definition(&mut self, origin: SourceLocation, reference: &SymbolReference, hold: Hold) {
        self.names.show(DefinitionCard::new(origin, reference), &self.session, self.capabilities.lookups, hold);
    }

    fn card_shows(&self, origin: &SourceLocation, reference: &SymbolReference) -> bool {
        self.names.card().is_some_and(|card| card.shows(origin, &reference.span.columns))
    }

    fn link_at(&mut self, position: Position) -> Option<Link> {
        let cell = self.patch_hits.code_cells.iter().find(|cell| cell.area.contains(position))?;
        let column = usize::from(position.x - cell.area.x);
        let pieces = row_pieces(&cell.text, cell.fit, cell.from);
        let byte = pieces.into_iter().find(|&(_, start, width)| column < start + width).map(|(byte, ..)| byte);
        let origin = cell.location.clone();
        let reference = byte.and_then(|byte| {
            self.symbols()
                .references_at(&origin)
                .iter()
                .find(|reference| reference.span.columns.contains(&byte))
                .cloned()
        });
        Some(Link { origin, reference })
    }

    pub fn lookup_finished(&mut self, reply: LookupReply) {
        self.mark_dirty();
        let index = self.symbols();
        match self.names.finish(reply, &mut self.session, &index) {
            Settled::Jump(target) => {
                self.names.close();
                let _ = self.handle_command(DiffReviewCommand::JumpTo(target));
            }
            Settled::Opened => self.request_follow(),
            Settled::Ignored | Settled::Redraw => {}
        }
    }

    pub(crate) fn reference_status(&self) -> Option<String> {
        self.session.reference_status()
    }

    pub(crate) fn name_overlays(&mut self) -> NameOverlays {
        let index = self.symbols();
        let card = self.names.card().map(|card| (card.origin.clone(), card.columns.clone()));
        NameOverlays { card, links: self.links_visible.then_some(index) }
    }

    pub(crate) fn symbol_path(&mut self, top: usize) -> Vec<String> {
        let presentation = self.presentation();
        let side = self.session.selected_side();
        let location =
            presentation.row(top).and_then(|row| presentation.source_location_from(row.file_index, top, side));
        location.map_or_else(Vec::new, |location| self.symbols().path_names(&location))
    }

    pub(crate) fn render_definition_card(&mut self, buffer: &mut Buffer, theme: &RatatuiTheme) {
        let Some(card) = self.names.card().cloned() else {
            return;
        };
        let area = self.hit_layout.patch;
        if area.is_empty() {
            return;
        }
        let index = self.symbols();
        let content = card.content(&index);
        let width = area.width.min(CARD_MAX_WIDTH).saturating_sub(CARD_CHROME);
        let card_lines = self.card_lines(&content, width, theme);
        let popup = placement(area, self.anchor(&card), &card_lines);
        self.hit_layout.card = Some(popup);
        let base = theme.ui.selection_style(SelectionState::None);
        Clear.render(popup, buffer);
        let block = Block::bordered()
            .padding(Padding::horizontal(1))
            .style(base)
            .border_style(Style::new().fg(theme.ui.border));
        let inner = block.inner(popup);
        block.render(popup, buffer);
        let CardLines { head, body, place, uses } = card_lines;
        let mut lines = head;
        if !lines.is_empty() {
            lines.push(Line::styled("─".repeat(usize::from(inner.width)), Style::new().fg(theme.ui.border)));
        }
        lines.extend(body);
        let [content_area, meta] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(1.min(inner.height))]).areas(inner);
        Paragraph::new(lines).style(base).render(content_area, buffer);
        let muted = base.fg(theme.ui.text_secondary);
        let uses_width = u16::try_from(uses.width()).unwrap_or(u16::MAX).min(meta.width);
        let [place_area, uses_area] =
            Layout::horizontal([Constraint::Min(0), Constraint::Length(uses_width)]).areas(meta);
        Paragraph::new(Span::styled(place, muted)).render(place_area, buffer);
        Paragraph::new(Span::styled(uses, muted)).render(uses_area, buffer);
    }

    fn anchor(&self, card: &DefinitionCard) -> Option<Position> {
        self.patch_hits.code_cells.iter().filter(|cell| cell.location == card.origin).find_map(|cell| {
            let pieces = row_pieces(&cell.text, cell.fit, cell.from);
            let (_, column, _) = pieces.into_iter().find(|&(offset, ..)| offset >= card.columns.start)?;
            let x = cell.area.x.saturating_add(u16::try_from(column).ok()?);
            Some(Position::new(x.min(cell.area.right().saturating_sub(1)), cell.area.y))
        })
    }

    fn card_lines(&mut self, content: &CardContent<'_>, width: u16, theme: &RatatuiTheme) -> CardLines {
        let secondary = Style::new().fg(theme.ui.text_secondary);
        let head = match (&content.message, &content.code) {
            (Some(message), _) => vec![Line::styled(message.clone(), secondary)],
            (None, Some((path, code))) => self.code_lines(path, code, theme),
            (None, None) => Vec::new(),
        };
        let mut body = Vec::new();
        if let Some(docs) = content.docs {
            let docs = self.doc_lines(docs, width);
            body.extend(docs.iter().take(DOC_LINES).cloned());
            if docs.len() > DOC_LINES {
                body.push(Line::styled("…", Style::new().fg(theme.ui.text_muted)));
            }
        }
        CardLines { head, body, place: content.place.clone().unwrap_or_default(), uses: content.uses.clone() }
    }

    fn doc_lines(&mut self, docs: &str, width: u16) -> Arc<[Line<'static>]> {
        let document = parse_markdown(docs, MarkdownOptions { intra_doc_links: true });
        let options = MarkdownLayoutOptions { width, heading_markers: false, ..MarkdownLayoutOptions::default() };
        MarkdownRenderer::new().render_lines(&document, options, &self.theme, &mut self.highlighter)
    }

    fn code_lines(&mut self, path: &RepoPath, code: &str, theme: &RatatuiTheme) -> Vec<Line<'static>> {
        let base = theme.ui.selection_style(SelectionState::None).fg(theme.ui.text);
        self.highlighter
            .with_theme(&self.theme.syntax)
            .highlight_lines(LanguageHint::Path(path.as_str()), code)
            .into_iter()
            .map(|(line, spans)| highlighted_line(line, &spans, base))
            .collect()
    }
}

struct CardLines {
    head: Vec<Line<'static>>,
    body: Vec<Line<'static>>,
    place: String,
    uses: String,
}

impl CardLines {
    fn width(&self) -> usize {
        let lines = self.head.iter().chain(&self.body).map(Line::width).max().unwrap_or(0);
        lines.max(self.place.width() + self.uses.width() + 2)
    }

    fn height(&self) -> usize {
        self.head.len() + usize::from(!self.head.is_empty()) + self.body.len() + 1
    }
}

fn placement(area: Rect, anchor: Option<Position>, lines: &CardLines) -> Rect {
    let width = u16::try_from(lines.width() + usize::from(CARD_CHROME))
        .unwrap_or(u16::MAX)
        .clamp(CARD_MIN_WIDTH, CARD_MAX_WIDTH)
        .min(area.width);
    let height = u16::try_from(lines.height() + 2).unwrap_or(u16::MAX).min(area.height);
    let Some(anchor) = anchor else {
        return Rect::new(area.x, area.bottom().saturating_sub(height), width, height);
    };
    let x = anchor.x.min(area.right().saturating_sub(width)).max(area.x);
    let y = if anchor.y + 1 + height <= area.bottom() {
        anchor.y + 1
    } else if anchor.y >= area.y + height {
        anchor.y - height
    } else {
        area.bottom().saturating_sub(height)
    };
    Rect::new(x, y, width, height)
}
