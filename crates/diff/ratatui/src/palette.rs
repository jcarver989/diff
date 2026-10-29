use crate::{
    BindingScope, DiffReviewCommand, DiffReviewEvent, DiffReviewState, InputOutcome, InteractionPhase, KeyBinding,
    KeyCode, KeyEvent, KeyModifiers, RatatuiTheme, ReviewCommand,
    interaction::is_plain_key,
    state::{Hit, Overlay},
    ui::{Modal, ModalSize, render_picker},
};
use clankerdiff_core::{CommandContext, CommandEntry, join_key_labels, palette_commands};
use clankerdiff_ui_core::{PickerOption, PickerState};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

const PAGE: isize = 8;

#[derive(Debug, Clone)]
pub(crate) struct CommandPalette {
    entries: Vec<(&'static CommandEntry, Option<String>)>,
    pub(crate) picker: PickerState,
}

impl CommandPalette {
    fn highlighted(&self) -> Option<&'static CommandEntry> {
        self.picker.highlighted().map(|index| self.entries[index].0)
    }
}

impl DiffReviewState {
    pub(crate) fn open_palette(&mut self) {
        let context = CommandContext { phase: InteractionPhase::Browse, ..self.command_context() };
        let entries: Vec<_> =
            palette_commands(&context, &self.session).map(|entry| (entry, self.key_label(&entry.command))).collect();
        let options = entries
            .iter()
            .map(|(entry, _)| {
                PickerOption::new(format!("{} {}", entry.title, entry.section.title())).group(entry.section.title())
            })
            .collect();
        self.overlay = Some(Overlay::Commands(CommandPalette { entries, picker: PickerState::new(options) }));
    }

    pub(crate) fn key_label(&self, command: &DiffReviewCommand) -> Option<String> {
        let navigation = self.command_context().navigation_available;
        let reachable = |binding: &&KeyBinding<DiffReviewCommand>| {
            &binding.command == command && (navigation || binding.scope != BindingScope::Navigation)
        };
        join_key_labels(self.keybindings.iter().filter(reachable).map(KeyBinding::key_label))
    }

    pub(crate) fn run_palette_entry(&mut self, command: DiffReviewCommand) -> InputOutcome<DiffReviewEvent> {
        self.overlay = None;
        self.mark_dirty();
        self.handle_command(command).or_consumed()
    }

    pub(crate) fn handle_palette_key(&mut self, key: KeyEvent) -> InputOutcome<DiffReviewEvent> {
        let Some(Overlay::Commands(palette)) = &mut self.overlay else {
            return InputOutcome::Ignored;
        };
        let picker = &mut palette.picker;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let step = match key.code {
            KeyCode::Esc => return self.handle_command(ReviewCommand::Cancel),
            KeyCode::Char('?') if picker.query().is_empty() => return self.handle_command(ReviewCommand::Cancel),
            KeyCode::Enter => {
                let Some(entry) = palette.highlighted() else {
                    return InputOutcome::Consumed;
                };
                return self.run_palette_entry(entry.command.clone());
            }
            KeyCode::Up => -1,
            KeyCode::Down => 1,
            KeyCode::Char('p' | 'k') if ctrl => -1,
            KeyCode::Char('n' | 'j') if ctrl => 1,
            KeyCode::PageUp => -PAGE,
            KeyCode::PageDown => PAGE,
            KeyCode::Backspace => {
                let mut query = picker.query().to_owned();
                query.pop();
                picker.set_query(query);
                0
            }
            KeyCode::Char(character) if is_plain_key(key) => {
                picker.set_query(format!("{}{character}", picker.query()));
                0
            }
            _ => return InputOutcome::Consumed,
        };
        picker.move_by(step);
        self.mark_dirty();
        InputOutcome::Consumed
    }
}

pub(crate) fn render_palette(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut DiffReviewState,
    theme: &RatatuiTheme,
) -> Option<Rect> {
    let Some(Overlay::Commands(palette)) = &state.overlay else {
        return None;
    };
    let content =
        Modal::new("Commands", ModalSize::Wide, theme).hint("↑/↓ move · Enter run · Esc close").render(area, buffer);
    let keys = Style::new().fg(theme.ui.text_muted);
    let entries = &palette.entries;
    let list =
        render_picker(content, buffer, theme, &palette.picker, Some(palette.picker.query()), state.pointer, |width| {
            entries
                .iter()
                .map(|(entry, label)| {
                    let label = label.as_deref().unwrap_or_default();
                    let gap = width.saturating_sub(entry.title.width() + label.width() + 2);
                    Line::from(vec![
                        Span::raw(format!(" {}{}", entry.title, " ".repeat(gap))),
                        Span::styled(format!("{label} "), keys),
                    ])
                })
                .collect()
        });
    state.cursor_position = list.cursor;
    let hits = list
        .rows
        .into_iter()
        .map(|(row, index)| Hit::new(row, entries[index].0.command.clone(), entries[index].0.title));
    state.hits.extend(hits);
    Some(content)
}
