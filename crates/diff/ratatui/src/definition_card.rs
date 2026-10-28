use crate::{
    DiffReviewCommand, DiffReviewEvent, DiffReviewState, InputOutcome, KeyCode, KeyEvent, MarkdownLayoutOptions,
    MarkdownRenderer, RatatuiTheme,
    interaction::is_plain_key,
    markdown::{MarkdownOptions, parse_markdown},
    syntax::highlighted_line,
    ui::SelectionState,
};
use clankerdiff_core::{LookupReply, RepoPath, SourceLocation};
use clankerdiff_symbols::{Activation, CardContent, DefinitionCard, Mark, NameHints, NamePick, Settled};
use clankerdiff_syntax::LanguageHint;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Clear, Padding, Paragraph, Widget},
};
use std::{ops::Range, sync::Arc};

const CARD_MIN_WIDTH: u16 = 40;
const CARD_MAX_WIDTH: u16 = 88;
const CARD_INDENT: u16 = 2;
const CARD_CHROME: u16 = 4;
const DOC_LINES: usize = 12;

pub(crate) type NameMarks = (SourceLocation, Vec<(Range<usize>, Mark)>);

impl DiffReviewState {
    pub(crate) fn show_definition_at_selection(&mut self) -> bool {
        let Some(origin) = self.session.selected_source_line() else {
            return false;
        };
        let index = self.symbols();
        self.names.open_at(origin, &self.session, &index, self.capabilities.lookups)
    }

    pub(crate) fn handle_lookup_key(&mut self, key: KeyEvent) -> Option<InputOutcome<DiffReviewEvent>> {
        let index = self.symbols();
        let enabled = self.capabilities.lookups;
        let Some(pick) = self.names.pick() else {
            let back = key.code == KeyCode::Esc && self.session.in_reference();
            return back.then(|| self.handle_command(DiffReviewCommand::NavigateBack));
        };
        if matches!(pick, NamePick::Hints(_)) {
            let picked = match key.code {
                KeyCode::Char(character) if is_plain_key(key) => {
                    self.names.pick_hint(character, &self.session, enabled)
                }
                _ => false,
            };
            if !picked {
                self.names.close();
            }
        } else if key.code == KeyCode::Esc || key == KeyCode::ctrl('g') {
            self.names.close();
        } else if key.code == KeyCode::Tab {
            self.names.cycle(&index);
        } else if key.code == KeyCode::Enter {
            match self.names.activate(&index) {
                Activation::Wait => {}
                Activation::Jump(target) => {
                    self.names.close();
                    return Some(self.handle_command(DiffReviewCommand::JumpTo(target)));
                }
                Activation::Idle => {
                    self.names.close();
                }
            }
        } else {
            return None;
        }
        self.mark_dirty();
        Some(InputOutcome::Consumed)
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

    pub(crate) fn name_marks(&mut self) -> Option<NameMarks> {
        let index = self.symbols();
        let origin = self.names.pick()?.origin().clone();
        let marks = self.names.marks(&origin, index.references_at(&origin));
        Some((origin, marks))
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
        let lines = self.card_lines(&content, width, theme);
        let popup = placement(area, self.origin_rows(&card), &lines);
        let hint = match (content.can_jump, content.can_cycle) {
            (true, true) => " Enter go · Tab next · Esc close ",
            (true, false) => " Enter go · Esc close ",
            (false, _) => " Esc close ",
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

    fn card_lines(&mut self, content: &CardContent<'_>, width: u16, theme: &RatatuiTheme) -> Vec<Line<'static>> {
        let muted = Style::new().fg(theme.ui.text_muted);
        let mut lines: Vec<Line<'static>> =
            content.message.iter().map(|message| Line::styled(message.clone(), muted)).collect();
        if let Some((path, code)) = &content.code {
            lines.extend(self.code_lines(path, code, theme));
        }
        if let Some(docs) = content.docs {
            if !lines.is_empty() {
                lines.push(Line::default());
            }
            let docs = self.doc_lines(docs, width);
            lines.extend(docs.iter().take(DOC_LINES).cloned());
            if docs.len() > DOC_LINES {
                lines.push(Line::styled("…", muted));
            }
        }
        if content.code.is_some() || content.docs.is_some() {
            lines.push(Line::default());
        }
        let footer = match &content.place {
            Some(place) => format!("{place} · {}", content.uses),
            None => content.uses.clone(),
        };
        lines.push(Line::styled(footer, muted));
        lines
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

pub(crate) fn hints_footer(hints: &NameHints) -> String {
    let names: Vec<String> = hints.keyed().map(|(key, reference)| format!("[{key}] {}", reference.name)).collect();
    format!("show definition: {}  [Esc] cancel", names.join("  "))
}

fn placement(area: Rect, origin: Option<(u16, u16)>, lines: &[Line<'_>]) -> Rect {
    let content = lines.iter().map(Line::width).max().unwrap_or(0);
    let width = u16::try_from(content + usize::from(CARD_CHROME))
        .unwrap_or(u16::MAX)
        .clamp(CARD_MIN_WIDTH, CARD_MAX_WIDTH)
        .min(area.width);
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX).min(area.height);
    let x = area.x + area.width.saturating_sub(width).min(CARD_INDENT);
    let y = match origin {
        Some((_, bottom)) if bottom + 1 + height <= area.bottom() => bottom + 1,
        Some((top, _)) if top >= area.y + height => top - height,
        _ => area.bottom().saturating_sub(height),
    };
    Rect::new(x, y, width, height)
}
