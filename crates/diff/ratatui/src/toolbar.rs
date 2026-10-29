use crate::{
    DiffReviewCommand, DiffReviewState, InteractionPhase, RatatuiTheme, RepositoryOperationStatus, ReviewCommand,
    state::Hit,
    style::stat_spans,
    ui::{ActionBar, Button, ButtonVariant},
};
use clankerdiff_core::{
    ControlKind, DRAFT_HINT, KeyState, Toolbar, ToolbarControl, browse_hint, command_entry, tooltip,
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

const MIN_SUBJECT_WIDTH: usize = 8;
const COMMANDS_HINT: &str = "? commands";

#[derive(Debug, Clone)]
enum Item {
    Button { label: String, command: DiffReviewCommand, tooltip: String, selected: bool, variant: ButtonVariant },
    Divider,
    Text(Vec<Span<'static>>),
}

impl Item {
    fn control(label: impl Into<String>, control: &ToolbarControl) -> Self {
        Self::Button {
            label: label.into(),
            command: control.command.clone(),
            tooltip: control.tooltip.clone(),
            selected: control.selected,
            variant: ButtonVariant::Ghost,
        }
    }

    fn width(&self) -> usize {
        match self {
            Self::Button { label, .. } => label.width() + 2,
            Self::Divider => 3,
            Self::Text(spans) => spans.iter().map(Span::width).sum(),
        }
    }
}

#[derive(Debug, Clone)]
struct Group {
    items: Vec<Item>,
    collapse: u8,
    right: bool,
    shrink: bool,
}

impl Group {
    const fn new(collapse: u8, items: Vec<Item>) -> Self {
        Self { items, collapse, right: false, shrink: false }
    }

    const fn right(mut self) -> Self {
        self.right = true;
        self
    }

    fn width(&self) -> usize {
        self.items.iter().map(Item::width).sum()
    }
}

pub(crate) fn render_toolbar(area: Rect, buffer: &mut Buffer, state: &mut DiffReviewState, theme: &RatatuiTheme) {
    if area.is_empty() {
        return;
    }
    buffer.set_style(area, Style::new().bg(theme.ui.canvas));
    let mut groups = toolbar_groups(&state.toolbar(), theme);
    fit(&mut groups, usize::from(area.width));
    let (right, left): (Vec<Group>, Vec<Group>) = groups.into_iter().partition(|group| group.right);
    let left_end = area.x.saturating_add(columns(left.iter().map(Group::width).sum()));
    let right_start = area.right().saturating_sub(columns(right.iter().map(Group::width).sum())).max(left_end);
    render_items(area, buffer, state, theme, left.into_iter().flat_map(|group| group.items), area.x, 0);
    render_items(area, buffer, state, theme, right.into_iter().flat_map(|group| group.items), right_start, 0);
}

pub(crate) fn render_review_bar(area: Rect, buffer: &mut Buffer, state: &mut DiffReviewState, theme: &RatatuiTheme) {
    if area.is_empty() {
        return;
    }
    let buttons = review_buttons(state);
    let buttons_width: usize = buttons.iter().map(|item| item.width() + 1).sum();
    let review = state.review();
    let outdated = review.outdated_count();
    let comments = format!(
        "{} review comment{}{}",
        review.len(),
        if review.len() == 1 { "" } else { "s" },
        if outdated == 0 { String::new() } else { format!(" ({outdated} outdated)") }
    );
    let hint = state.pointer.and_then(|pointer| state.hit_at(pointer)).map_or_else(
        || {
            if matches!(state.repository_status, RepositoryOperationStatus::Pending) {
                "Git operation in progress…".to_owned()
            } else if state.session.draft().is_some() {
                DRAFT_HINT.to_owned()
            } else {
                browse_hint(state.key_state(), state.session.source_view().is_some()).to_owned()
            }
        },
        |hit| hit.tooltip.clone(),
    );
    let text_width = usize::from(area.width).saturating_sub(buttons_width);
    let comments = format!("    ·    {comments}");
    let fallback = if state.interaction_phase() == InteractionPhase::Browse { COMMANDS_HINT } else { hint.as_str() };
    let (hint, comments) = [(hint.as_str(), comments.as_str()), (fallback, comments.as_str()), (fallback, "")]
        .into_iter()
        .find(|(hint, comments)| hint.width() + comments.width() < text_width)
        .unwrap_or((fallback, ""));
    ActionBar::new(
        Line::from(vec![
            Span::styled(format!(" {hint}"), Style::new().fg(theme.ui.text_muted)),
            Span::styled(comments.to_owned(), Style::new().fg(theme.ui.accent)),
        ]),
        theme,
    )
    .render(Rect::new(area.x, area.y, area.width, 1), buffer);
    render_items(area, buffer, state, theme, buttons, area.x.saturating_add(columns(text_width)), 1);
}

impl DiffReviewState {
    pub(crate) fn key_state(&self) -> KeyState {
        KeyState::new(self.focus, &self.session, self.names.card().is_some())
    }

    fn toolbar(&self) -> Toolbar {
        let sidebar = self.command_context().navigation_available.then_some(self.sidebar_hidden);
        Toolbar::new(&self.session, sidebar, |command| self.key_label(command))
    }
}

fn render_items(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut DiffReviewState,
    theme: &RatatuiTheme,
    items: impl IntoIterator<Item = Item>,
    mut x: u16,
    gap: u16,
) {
    for item in items {
        let cell = Rect::new(x, area.y, columns(item.width()).min(area.right().saturating_sub(x)), 1);
        x = cell.right().saturating_add(gap);
        match item {
            Item::Button { label, command, tooltip, selected, variant } => {
                let enabled = state.command_enabled(&command);
                Button::new(&label, theme)
                    .variant(variant)
                    .selected(selected)
                    .disabled(!enabled)
                    .hovered_at(cell, state.pointer)
                    .render(cell, buffer);
                if enabled {
                    state.hits.push(Hit::new(cell, command, tooltip));
                }
            }
            Item::Divider => Paragraph::new(" │ ").style(Style::new().fg(theme.ui.border)).render(cell, buffer),
            Item::Text(spans) => Paragraph::new(Line::from(spans)).render(cell, buffer),
        }
    }
}

fn toolbar_groups(toolbar: &Toolbar, theme: &RatatuiTheme) -> Vec<Group> {
    let button = |control: &ToolbarControl| Item::control(glyph(control.kind), control);
    let mut leading: Vec<Item> = toolbar.files.iter().map(button).collect();
    if !leading.is_empty() {
        leading.push(Item::Divider);
    }
    leading.extend(toolbar.history.iter().map(button));
    let has_files = !toolbar.history.is_empty();
    let mut groups = vec![Group::new(0, leading)];
    if has_files && (toolbar.page.is_some() || !toolbar.scopes.is_empty()) {
        groups.push(Group::new(0, vec![Item::Divider]));
    }
    if let (Some(label), [previous, next, choose]) = (&toolbar.page, toolbar.stepper.as_slice()) {
        groups.push(Group::new(0, vec![button(previous), button(next)]));
        let summary = [label.short_id(), label.subject()].into_iter().flatten().collect::<Vec<_>>().join(" ");
        groups.push(Group { shrink: true, ..Group::new(3, vec![Item::control(format!("{summary} ▾"), choose)]) });
        if let Some(position) = label.position_label() {
            let position = Span::styled(format!("{position} "), Style::new().fg(theme.ui.text_muted));
            groups.push(Group::new(2, vec![Item::Text(vec![position])]));
        }
    }
    if !toolbar.scopes.is_empty() {
        let divider = toolbar.page.is_some().then_some(Item::Divider);
        groups.push(Group::new(5, divider.into_iter().chain(toolbar.scopes.iter().map(button)).collect()));
    }
    groups.push(Group::new(4, toolbar.layouts.iter().map(button).collect()).right());
    if let Some(stat) = toolbar.stat {
        let spans = [vec![Span::raw("  ")], stat_spans(stat, theme).to_vec(), vec![Span::raw(" ")]];
        groups.push(Group::new(1, vec![Item::Text(spans.concat())]).right());
    }
    groups
}

fn glyph(kind: ControlKind) -> String {
    match kind {
        ControlKind::Sidebar { hidden: true } => "▸".to_owned(),
        ControlKind::Sidebar { hidden: false } => "◂".to_owned(),
        ControlKind::AllFiles { collapsed: true } => "⊞".to_owned(),
        ControlKind::AllFiles { collapsed: false } => "⊟".to_owned(),
        ControlKind::Back => "←".to_owned(),
        ControlKind::Forward => "→".to_owned(),
        ControlKind::PreviousCommit => "‹".to_owned(),
        ControlKind::NextCommit => "›".to_owned(),
        ControlKind::ChooseCommit => "▾".to_owned(),
        ControlKind::Scope(scope) => scope.title().to_owned(),
        ControlKind::Layout(mode) => format!("{mode:?}"),
    }
}

fn fit(groups: &mut Vec<Group>, width: usize) {
    loop {
        let excess = (groups.iter().map(Group::width).sum::<usize>() + 1).saturating_sub(width);
        let collapsible = groups.iter().enumerate().filter(|(_, group)| group.collapse > 0);
        let Some(index) =
            collapsible.min_by_key(|(_, group)| group.collapse).map(|(index, _)| index).filter(|_| excess > 0)
        else {
            return;
        };
        if !(groups[index].shrink && shorten(&mut groups[index].items, excess)) {
            groups.remove(index);
        }
    }
}

fn shorten(items: &mut [Item], excess: usize) -> bool {
    let [Item::Button { label, .. }] = items else {
        return false;
    };
    let text = label.trim_end_matches(" ▾").trim_end_matches('…');
    let keep = text.chars().count().saturating_sub(excess + 1).max(MIN_SUBJECT_WIDTH);
    let shortened = format!("{}… ▾", text.chars().take(keep).collect::<String>());
    if shortened.width() >= label.width() {
        return false;
    }
    *label = shortened;
    true
}

fn review_buttons(state: &DiffReviewState) -> Vec<Item> {
    let action = |label: &str, command: DiffReviewCommand, variant| {
        let title = command_entry(&command).map_or(label, |entry| entry.title);
        let tooltip = tooltip(title, state.key_label(&command));
        Item::Button { label: label.to_owned(), command, tooltip, selected: false, variant }
    };
    let mut buttons = Vec::new();
    if state.command_context().themes_available {
        buttons.push(action("Theme", ReviewCommand::OpenThemePicker.into(), ButtonVariant::Ghost));
    }
    if !state.review().is_empty() {
        buttons.push(action("Copy", DiffReviewCommand::CopyReview, ButtonVariant::Secondary));
        buttons.push(action("Submit", DiffReviewCommand::SubmitReview, ButtonVariant::Primary));
    }
    buttons.push(action("Cancel", ReviewCommand::Cancel.into(), ButtonVariant::Ghost));
    buttons
}

fn columns(width: usize) -> u16 {
    u16::try_from(width).unwrap_or(u16::MAX)
}
