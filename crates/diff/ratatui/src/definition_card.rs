use crate::{
    DiffReviewCommand, DiffReviewEvent, DiffReviewState, InputOutcome, KeyCode, KeyEvent, RatatuiTheme,
    interaction::is_plain_key, syntax::highlighted_line, ui::SelectionState,
};
use clankerdiff_core::SourceDocument;
use clankerdiff_symbols::{Definition, DefinitionCard, Lookup, NameHints, SymbolIndex};
use clankerdiff_syntax::LanguageHint;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Clear, Padding, Paragraph, Widget},
};

const CARD_MIN_WIDTH: u16 = 40;
const CARD_MAX_WIDTH: u16 = 88;
const CARD_INDENT: u16 = 2;
const DOC_LINES: usize = 12;

impl DiffReviewState {
    pub(crate) fn show_definition_at_selection(&mut self) -> bool {
        let Some(origin) = self.session.selected_source_line() else {
            return false;
        };
        self.lookup = Lookup::at(&self.symbols(), origin);
        self.lookup.is_some()
    }

    pub(crate) fn handle_lookup_key(&mut self, key: KeyEvent) -> Option<InputOutcome<DiffReviewEvent>> {
        let index = self.symbols();
        let cancel = key.code == KeyCode::Esc || key == KeyCode::ctrl('g');
        match self.lookup.as_mut()? {
            Lookup::Hints(hints) => {
                let picked = match key.code {
                    KeyCode::Char(character) if is_plain_key(key) => hints.pick(character),
                    _ => None,
                };
                self.lookup = picked.map(Lookup::Card);
            }
            Lookup::Card(_) if cancel => self.lookup = None,
            Lookup::Card(card) if key.code == KeyCode::Tab => {
                card.cycle(&index);
            }
            Lookup::Card(card) if key.code == KeyCode::Enter => {
                let target = card.target(&index);
                self.lookup = None;
                if let Some(target) = target {
                    return Some(self.handle_command(DiffReviewCommand::JumpTo(target)));
                }
            }
            Lookup::Card(_) => return None,
        }
        self.mark_dirty();
        Some(InputOutcome::Consumed)
    }

    pub(crate) fn render_definition_card(&mut self, buffer: &mut Buffer, theme: &RatatuiTheme) {
        let Some(Lookup::Card(card)) = self.lookup.clone() else {
            return;
        };
        let area = self.hit_layout.patch;
        if area.is_empty() {
            return;
        }
        let index = self.symbols();
        let lines = self.card_lines(&card, &index, theme);
        let popup = placement(area, self.origin_rows(&card), &lines);
        let hint = match card.choice(&index) {
            Some(choice) if choice.count > 1 => " Enter go · Tab next · Esc close ",
            Some(_) => " Enter go · Esc close ",
            None => " Esc close ",
        };
        Clear.render(popup, buffer);
        let block = Block::bordered()
            .title(format!(" {} ", card.name))
            .title_bottom(hint)
            .padding(Padding::horizontal(1))
            .style(theme.ui.selection_style(SelectionState::None))
            .border_style(Style::new().fg(theme.ui.accent));
        let inner = block.inner(popup);
        block.render(popup, buffer);
        Paragraph::new(lines).render(inner, buffer);
    }

    fn origin_rows(&self, card: &DefinitionCard) -> Option<(u16, u16)> {
        let row = self.presentation().row_showing_source(&card.origin)?;
        let rows = self.visible_rows.iter().filter(|(_, index)| *index == row).map(|(y, _)| *y);
        let (top, bottom) = rows.fold((u16::MAX, 0), |(top, bottom), y| (top.min(y), bottom.max(y)));
        (top <= bottom).then_some((top, bottom))
    }

    fn card_lines(&mut self, card: &DefinitionCard, index: &SymbolIndex, theme: &RatatuiTheme) -> Vec<Line<'static>> {
        let muted = Style::new().fg(theme.ui.text_muted);
        let uses = format!("{} in this diff", uses_label(index.uses(&card.name).len()));
        let Some(choice) = card.choice(index) else {
            return vec![
                Line::styled(format!("No definition of {} in this diff", card.name), muted),
                Line::styled(uses, muted),
            ];
        };
        let mut lines = self.signature_lines(choice.definition, theme);
        if let Some(docs) = &choice.definition.docs {
            lines.push(Line::default());
            let mut docs = docs.lines();
            let text = Style::new().fg(theme.ui.text_secondary);
            lines.extend(docs.by_ref().take(DOC_LINES).map(|line| Line::styled(line.to_owned(), text)));
            if docs.next().is_some() {
                lines.push(Line::styled("…", muted));
            }
        }
        let location = &choice.definition.location;
        let position = match choice.count {
            0 | 1 => String::new(),
            count => format!(" · {} of {count}", choice.position + 1),
        };
        lines.push(Line::default());
        lines.push(Line::styled(format!("{}:{}{position} · {uses}", location.path, location.line_number), muted));
        lines
    }

    fn signature_lines(&mut self, definition: &Definition, theme: &RatatuiTheme) -> Vec<Line<'static>> {
        let base = theme.ui.selection_style(SelectionState::None).fg(theme.ui.text);
        let Ok(source) = SourceDocument::new(&definition.signature) else {
            return vec![Line::styled(definition.signature.clone(), base)];
        };
        let highlights = self
            .highlighter
            .with_theme(&self.theme.syntax)
            .highlight_document(source.content_id(), LanguageHint::Path(definition.location.path.as_str()), || {
                source.text()
            })
            .ok();
        (1..=source.line_count())
            .map(|line_number| {
                let text = source.line(line_number).unwrap_or_default();
                let spans = highlights.as_ref().and_then(|highlights| highlights.line_shared(line_number - 1));
                highlighted_line(text, spans.as_deref().unwrap_or_default(), base)
            })
            .collect()
    }
}

pub(crate) fn hints_footer(hints: &NameHints) -> String {
    let names: Vec<String> = hints.keyed().map(|(key, reference)| format!("[{key}] {}", reference.name)).collect();
    format!("show definition: {}  [Esc] cancel", names.join("  "))
}

fn placement(area: Rect, origin: Option<(u16, u16)>, lines: &[Line<'_>]) -> Rect {
    let content = lines.iter().map(Line::width).max().unwrap_or(0);
    let width = u16::try_from(content + 4).unwrap_or(u16::MAX).clamp(CARD_MIN_WIDTH, CARD_MAX_WIDTH).min(area.width);
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX).min(area.height);
    let x = area.x + area.width.saturating_sub(width).min(CARD_INDENT);
    let y = match origin {
        Some((_, bottom)) if bottom + 1 + height <= area.bottom() => bottom + 1,
        Some((top, _)) if top >= area.y + height => top - height,
        _ => area.bottom().saturating_sub(height),
    };
    Rect::new(x, y, width, height)
}

fn uses_label(count: usize) -> String {
    if count == 1 { "1 use".to_owned() } else { format!("{count} uses") }
}
